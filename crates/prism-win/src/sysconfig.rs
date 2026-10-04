//! Services (SCM) et stratégies (registre HKLM) pour l'allègement.

use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::{null, null_mut};

use prism_core::allege::{StartType, SystemConfig};

use windows_sys::Win32::Foundation::{
    GetLastError, ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, ERROR_INSUFFICIENT_BUFFER, ERROR_SERVICE_DOES_NOT_EXIST,
};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_DWORD, REG_OPTION_NON_VOLATILE, REG_VALUE_TYPE,
};
use windows_sys::Win32::System::Services::{
    ChangeServiceConfig2W, ChangeServiceConfigW, CloseServiceHandle, ControlService, OpenSCManagerW, OpenServiceW,
    QueryServiceConfig2W, QueryServiceConfigW, StartServiceW, QUERY_SERVICE_CONFIGW, SC_HANDLE, SC_MANAGER_CONNECT,
    SERVICE_AUTO_START, SERVICE_BOOT_START, SERVICE_CHANGE_CONFIG, SERVICE_CONFIG_DELAYED_AUTO_START_INFO,
    SERVICE_CONTROL_STOP, SERVICE_DELAYED_AUTO_START_INFO, SERVICE_DEMAND_START, SERVICE_DISABLED, SERVICE_NO_CHANGE,
    SERVICE_QUERY_CONFIG, SERVICE_START, SERVICE_STATUS, SERVICE_STOP, SERVICE_SYSTEM_START,
};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn win_err(what: &str, code: u32) -> String {
    if code == ERROR_ACCESS_DENIED {
        format!("{what} : droits administrateur requis")
    } else {
        format!("{what} : erreur Windows {code}")
    }
}

struct Sc(SC_HANDLE);

impl Drop for Sc {
    fn drop(&mut self) {
        // SAFETY: handle valide possédé par cette structure.
        unsafe { CloseServiceHandle(self.0) };
    }
}

struct Key(HKEY);

impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: clé ouverte possédée par cette structure.
        unsafe { RegCloseKey(self.0) };
    }
}

/// `Ok(None)` si le service n'existe pas.
fn open_service(name: &str, access: u32) -> Result<Option<(Sc, Sc)>, String> {
    // SAFETY: chaînes larges terminées par zéro, handles vérifiés puis possédés.
    unsafe {
        let scm = OpenSCManagerW(null(), null(), SC_MANAGER_CONNECT);
        if scm.is_null() {
            return Err(win_err("gestionnaire de services", GetLastError()));
        }
        let scm = Sc(scm);
        let n = wide(name);
        let svc = OpenServiceW(scm.0, n.as_ptr(), access);
        if svc.is_null() {
            return match GetLastError() {
                ERROR_SERVICE_DOES_NOT_EXIST => Ok(None),
                code => Err(win_err(name, code)),
            };
        }
        Ok(Some((Sc(svc), scm)))
    }
}

fn read_start(svc: &Sc) -> Result<StartType, String> {
    // SAFETY: premier appel pour connaître la taille, second avec un tampon assez grand
    // et aligné (Vec<u64>).
    unsafe {
        let mut needed = 0u32;
        QueryServiceConfigW(svc.0, null_mut(), 0, &mut needed);
        if GetLastError() != ERROR_INSUFFICIENT_BUFFER {
            return Err(win_err("lecture du service", GetLastError()));
        }
        let mut buf = vec![0u64; (needed as usize).div_ceil(8)];
        let cfg = buf.as_mut_ptr() as *mut QUERY_SERVICE_CONFIGW;
        if QueryServiceConfigW(svc.0, cfg, needed, &mut needed) == 0 {
            return Err(win_err("lecture du service", GetLastError()));
        }
        let start = (*cfg).dwStartType;
        Ok(match start {
            SERVICE_BOOT_START => StartType::Boot,
            SERVICE_SYSTEM_START => StartType::System,
            SERVICE_AUTO_START => {
                let mut info = SERVICE_DELAYED_AUTO_START_INFO { fDelayedAutostart: 0 };
                let mut n = 0u32;
                let ok = QueryServiceConfig2W(
                    svc.0,
                    SERVICE_CONFIG_DELAYED_AUTO_START_INFO,
                    &mut info as *mut _ as *mut u8,
                    size_of::<SERVICE_DELAYED_AUTO_START_INFO>() as u32,
                    &mut n,
                );
                if ok != 0 && info.fDelayedAutostart != 0 {
                    StartType::AutoDelayed
                } else {
                    StartType::Auto
                }
            }
            SERVICE_DEMAND_START => StartType::Manual,
            SERVICE_DISABLED => StartType::Disabled,
            other => return Err(format!("mode de démarrage inconnu {other}")),
        })
    }
}

pub struct WindowsSystemConfig;

impl SystemConfig for WindowsSystemConfig {
    fn service_start(&mut self, name: &str) -> Result<Option<StartType>, String> {
        match open_service(name, SERVICE_QUERY_CONFIG)? {
            None => Ok(None),
            Some((svc, _scm)) => read_start(&svc).map(Some),
        }
    }

    fn set_service_start(&mut self, name: &str, to: StartType) -> Result<(), String> {
        let access = SERVICE_QUERY_CONFIG | SERVICE_CHANGE_CONFIG | SERVICE_STOP | SERVICE_START;
        let Some((svc, _scm)) = open_service(name, access)? else {
            return Err("service absent".into());
        };
        let raw = match to {
            StartType::Boot => SERVICE_BOOT_START,
            StartType::System => SERVICE_SYSTEM_START,
            StartType::Auto | StartType::AutoDelayed => SERVICE_AUTO_START,
            StartType::Manual => SERVICE_DEMAND_START,
            StartType::Disabled => SERVICE_DISABLED,
        };
        // SAFETY: handle vérifié ; tous les autres paramètres sont « inchangés ».
        unsafe {
            if ChangeServiceConfigW(
                svc.0,
                SERVICE_NO_CHANGE,
                raw,
                SERVICE_NO_CHANGE,
                null(),
                null(),
                null_mut(),
                null(),
                null(),
                null(),
                null(),
            ) == 0
            {
                return Err(win_err(name, GetLastError()));
            }
            if matches!(to, StartType::Auto | StartType::AutoDelayed) {
                let info = SERVICE_DELAYED_AUTO_START_INFO {
                    fDelayedAutostart: (to == StartType::AutoDelayed) as i32,
                };
                ChangeServiceConfig2W(
                    svc.0,
                    SERVICE_CONFIG_DELAYED_AUTO_START_INFO,
                    &info as *const _ as *const c_void,
                );
                // Remis en automatique : on le relance tout de suite (sans attendre un redémarrage).
                StartServiceW(svc.0, 0, null());
            }
            if to == StartType::Disabled {
                // Désactivé : on l'arrête aussi, pour un effet immédiat. Échec sans gravité
                // (déjà arrêté, ou dépendances) : il ne redémarrera plus.
                let mut status: SERVICE_STATUS = std::mem::zeroed();
                ControlService(svc.0, SERVICE_CONTROL_STOP, &mut status);
            }
        }
        Ok(())
    }

    fn policy(&mut self, key: &str, value: &str) -> Result<Option<u32>, String> {
        let k = wide(key);
        let v = wide(value);
        // SAFETY: chaînes larges terminées par zéro, sorties locales dimensionnées.
        unsafe {
            let mut h: HKEY = null_mut();
            match RegOpenKeyExW(HKEY_LOCAL_MACHINE, k.as_ptr(), 0, KEY_QUERY_VALUE, &mut h) {
                0 => {}
                ERROR_FILE_NOT_FOUND => return Ok(None),
                code => return Err(win_err(key, code)),
            }
            let h = Key(h);
            let mut kind: REG_VALUE_TYPE = 0;
            let mut data = 0u32;
            let mut len = size_of::<u32>() as u32;
            match RegQueryValueExW(
                h.0,
                v.as_ptr(),
                null(),
                &mut kind,
                &mut data as *mut u32 as *mut u8,
                &mut len,
            ) {
                0 if kind == REG_DWORD => Ok(Some(data)),
                0 => Err(format!("{key}\\{value} : valeur non DWORD")),
                ERROR_FILE_NOT_FOUND => Ok(None),
                code => Err(win_err(value, code)),
            }
        }
    }

    fn set_policy(&mut self, key: &str, value: &str, data: u32) -> Result<(), String> {
        let k = wide(key);
        let v = wide(value);
        // SAFETY: chaînes larges terminées par zéro ; la donnée est un u32 local.
        unsafe {
            let mut h: HKEY = null_mut();
            let st = RegCreateKeyExW(
                HKEY_LOCAL_MACHINE,
                k.as_ptr(),
                0,
                null(),
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE,
                null(),
                &mut h,
                null_mut(),
            );
            if st != 0 {
                return Err(win_err(key, st));
            }
            let h = Key(h);
            let bytes = data.to_le_bytes();
            match RegSetValueExW(h.0, v.as_ptr(), 0, REG_DWORD, bytes.as_ptr(), 4) {
                0 => Ok(()),
                code => Err(win_err(value, code)),
            }
        }
    }

    fn delete_policy(&mut self, key: &str, value: &str) -> Result<(), String> {
        let k = wide(key);
        let v = wide(value);
        // SAFETY: chaînes larges terminées par zéro.
        unsafe {
            let mut h: HKEY = null_mut();
            match RegOpenKeyExW(HKEY_LOCAL_MACHINE, k.as_ptr(), 0, KEY_SET_VALUE, &mut h) {
                0 => {}
                ERROR_FILE_NOT_FOUND => return Ok(()),
                code => return Err(win_err(key, code)),
            }
            let h = Key(h);
            match RegDeleteValueW(h.0, v.as_ptr()) {
                0 | ERROR_FILE_NOT_FOUND => Ok(()),
                code => Err(win_err(value, code)),
            }
        }
    }
}
