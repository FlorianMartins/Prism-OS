//! Services (SCM) et stratégies (registre HKLM) pour l'allègement.

use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::{null, null_mut};

use prism_core::allege::{RegData, StartType, SystemConfig};

use windows_sys::Win32::Foundation::{
    GetLastError, ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, ERROR_INSUFFICIENT_BUFFER, ERROR_SERVICE_DOES_NOT_EXIST,
};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_DWORD, REG_OPTION_NON_VOLATILE, REG_SZ,
    REG_VALUE_TYPE,
};
use windows_sys::Win32::System::Services::{
    ChangeServiceConfig2W, ChangeServiceConfigW, CloseServiceHandle, ControlService, OpenSCManagerW, OpenServiceW,
    QueryServiceConfig2W, QueryServiceConfigW, StartServiceW, QUERY_SERVICE_CONFIGW, SC_HANDLE, SC_MANAGER_CONNECT,
    SERVICE_AUTO_START, SERVICE_BOOT_START, SERVICE_CHANGE_CONFIG, SERVICE_CONFIG_DELAYED_AUTO_START_INFO,
    SERVICE_CONTROL_STOP, SERVICE_DELAYED_AUTO_START_INFO, SERVICE_DEMAND_START, SERVICE_DISABLED, SERVICE_NO_CHANGE,
    SERVICE_QUERY_CONFIG, SERVICE_QUERY_STATUS, SERVICE_START, SERVICE_STATUS, SERVICE_STOP, SERVICE_SYSTEM_START,
};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn win_err(what: &str, code: u32) -> String {
    if code == ERROR_ACCESS_DENIED {
        format!("{what} : accès refusé (droits administrateur requis, ou réglage protégé par Windows)")
    } else {
        format!("{what} : erreur Windows {code}")
    }
}

/// Sépare `HKLM\\…` / `HKCU\\…` en (racine, sous-clé).
fn split_hive(full: &str) -> Result<(HKEY, &str), String> {
    let (hive, sub) = full
        .split_once('\\')
        .ok_or_else(|| format!("{full} : chemin sans ruche"))?;
    let root = match hive.to_ascii_uppercase().as_str() {
        "HKLM" => HKEY_LOCAL_MACHINE,
        "HKCU" => HKEY_CURRENT_USER,
        other => return Err(format!("ruche inconnue {other}")),
    };
    Ok((root, sub))
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
    fn task_enabled(&mut self, path: &str) -> Result<Option<bool>, String> {
        let out = schtasks(&["/Query", "/TN", path, "/XML"])?;
        if !out.status.success() {
            // Tâche inconnue : schtasks échoue (message traduit, on ne le lit pas).
            return Ok(None);
        }
        Ok(Some(crate::task_xml_enabled(&decode_console(&out.stdout))))
    }

    fn set_task_enabled(&mut self, path: &str, enabled: bool) -> Result<(), String> {
        let flag = if enabled { "/ENABLE" } else { "/DISABLE" };
        let out = schtasks(&["/Change", "/TN", path, flag])?;
        if out.status.success() {
            Ok(())
        } else {
            Err(format!("{path} : schtasks a refusé (droits administrateur ?)"))
        }
    }

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

    fn policy(&mut self, key: &str, value: &str) -> Result<Option<RegData>, String> {
        reg_get(key, value)
    }

    fn set_policy(&mut self, key: &str, value: &str, data: &RegData) -> Result<(), String> {
        // Dernière barrière : hors liste autorisée, rien n'est écrit.
        if !prism_core::allege::registry_allowed(key) {
            return Err(format!("{key} : clé hors de la liste autorisée"));
        }
        reg_set(key, value, data)
    }

    fn delete_policy(&mut self, key: &str, value: &str) -> Result<(), String> {
        if !prism_core::allege::registry_allowed(key) {
            return Err(format!("{key} : clé hors de la liste autorisée"));
        }
        reg_delete(key, value)
    }
}

/// PID du processus d'un service en cours d'exécution.
pub(crate) fn service_pid(name: &str) -> Option<u32> {
    use windows_sys::Win32::System::Services::{QueryServiceStatusEx, SC_STATUS_PROCESS_INFO, SERVICE_STATUS_PROCESS};
    let (svc, _scm) = open_service(name, SERVICE_QUERY_STATUS).ok()??;
    // SAFETY: structure de sortie de taille exacte.
    unsafe {
        let mut st: SERVICE_STATUS_PROCESS = std::mem::zeroed();
        let mut n = 0u32;
        let ok = QueryServiceStatusEx(
            svc.0,
            SC_STATUS_PROCESS_INFO,
            &mut st as *mut _ as *mut u8,
            size_of::<SERVICE_STATUS_PROCESS>() as u32,
            &mut n,
        );
        (ok != 0 && st.dwProcessId != 0).then_some(st.dwProcessId)
    }
}

/// Arrête un service en cours d'exécution ; rend de quoi le relancer.
pub(crate) fn pause_service(name: &str) -> prism_core::platform::Outcome {
    use prism_core::journal::Undo;
    use prism_core::platform::Outcome;
    use windows_sys::Win32::System::Services::{QueryServiceStatus, SERVICE_QUERY_STATUS, SERVICE_RUNNING};
    let svc = match open_service(name, SERVICE_QUERY_STATUS | SERVICE_STOP) {
        Ok(Some((svc, _scm))) => svc,
        Ok(None) => return Outcome::Skipped("service absent".into()),
        Err(e) => return Outcome::Skipped(e),
    };
    // SAFETY: handle vérifié, structure de sortie locale.
    unsafe {
        let mut status: SERVICE_STATUS = std::mem::zeroed();
        if QueryServiceStatus(svc.0, &mut status) == 0 {
            return Outcome::Failed(win_err(name, GetLastError()));
        }
        if status.dwCurrentState != SERVICE_RUNNING {
            return Outcome::Skipped("déjà arrêté".into());
        }
        if ControlService(svc.0, SERVICE_CONTROL_STOP, &mut status) == 0 {
            return Outcome::Skipped(win_err(name, GetLastError()));
        }
    }
    Outcome::Done(Some(Undo::Service { name: name.to_string() }))
}

/// Relance un service mis en pause par Prism.
pub(crate) fn resume_service(name: &str) -> prism_core::platform::Outcome {
    use prism_core::platform::Outcome;
    use windows_sys::Win32::Foundation::ERROR_SERVICE_ALREADY_RUNNING;
    let svc = match open_service(name, SERVICE_START) {
        Ok(Some((svc, _scm))) => svc,
        Ok(None) => return Outcome::Skipped("service absent".into()),
        Err(e) => return Outcome::Failed(e),
    };
    // SAFETY: handle vérifié.
    unsafe {
        if StartServiceW(svc.0, 0, std::ptr::null()) == 0 {
            let code = GetLastError();
            if code != ERROR_SERVICE_ALREADY_RUNNING {
                return Outcome::Failed(win_err(name, code));
            }
        }
    }
    Outcome::Done(None)
}

/// Le processus courant tourne-t-il avec un jeton élevé (administrateur) ?
pub fn is_elevated() -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    // SAFETY: jeton du processus courant, sortie locale de taille exacte, handle fermé.
    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut elev = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut len = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut elev as *mut _ as *mut core::ffi::c_void,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        );
        CloseHandle(token);
        ok != 0 && elev.TokenIsElevated != 0
    }
}

/// Relance l'exécutable courant en administrateur (fenêtre UAC). `Ok` si lancé.
pub fn relaunch_elevated() -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let file: Vec<u16> = exe
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let verb: Vec<u16> = "runas".encode_utf16().chain(Some(0)).collect();
    // Les arguments suivent (ex. `prism-setup --auto <dossier>`).
    let args: Vec<String> = std::env::args()
        .skip(1)
        .map(|a| format!("\"{}\"", a.replace('"', "")))
        .collect();
    let params: Vec<u16> = args.join(" ").encode_utf16().chain(Some(0)).collect();
    // SAFETY: chaînes larges terminées par zéro ; pas de fenêtre parente.
    let r = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            params.as_ptr(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if r as isize > 32 {
        Ok(())
    } else {
        Err("élévation refusée ou impossible".into())
    }
}

fn schtasks(args: &[&str]) -> Result<std::process::Output, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    std::process::Command::new("schtasks.exe")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("schtasks.exe : {e}"))
}

/// Sortie console : UTF-16 (avec ou sans marque) ou UTF-8 selon les cas.
pub(crate) fn decode_console(bytes: &[u8]) -> String {
    let utf16 = bytes.len() >= 2 && (bytes.starts_with(&[0xff, 0xfe]) || bytes[1] == 0);
    if utf16 {
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&units)
            .trim_start_matches('\u{feff}')
            .to_string()
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

/// Lecture / écriture du registre sans contrôle de liste : chaque appelant
/// (allègement, apparence) applique sa propre liste autorisée.
pub(crate) fn reg_get(key: &str, value: &str) -> Result<Option<RegData>, String> {
    let (root, sub) = split_hive(key)?;
    let k = wide(sub);
    let v = wide(value);
    // SAFETY: chaînes larges terminées par zéro, tampons locaux dimensionnés.
    unsafe {
        let mut h: HKEY = null_mut();
        match RegOpenKeyExW(root, k.as_ptr(), 0, KEY_QUERY_VALUE, &mut h) {
            0 => {}
            ERROR_FILE_NOT_FOUND => return Ok(None),
            code => return Err(win_err(key, code)),
        }
        let h = Key(h);
        let mut kind: REG_VALUE_TYPE = 0;
        let mut buf = [0u16; 1024];
        let mut len = (buf.len() * 2) as u32;
        match RegQueryValueExW(
            h.0,
            v.as_ptr(),
            null(),
            &mut kind,
            buf.as_mut_ptr() as *mut u8,
            &mut len,
        ) {
            0 if kind == REG_DWORD && len == 4 => Ok(Some(RegData::Dword(u32::from_le_bytes(
                (*(buf.as_ptr() as *const [u8; 4])).to_owned(),
            )))),
            0 if kind == REG_SZ => {
                let n = (len as usize / 2).min(buf.len());
                let text = String::from_utf16_lossy(&buf[..n]);
                Ok(Some(RegData::Text(text.trim_end_matches('\0').to_string())))
            }
            0 => Err(format!("{key}\\{value} : type de valeur inattendu ({kind})")),
            ERROR_FILE_NOT_FOUND => Ok(None),
            code => Err(win_err(value, code)),
        }
    }
}

pub(crate) fn reg_set(key: &str, value: &str, data: &RegData) -> Result<(), String> {
    let (root, sub) = split_hive(key)?;
    let k = wide(sub);
    let v = wide(value);
    // SAFETY: chaînes larges terminées par zéro ; données locales de taille exacte.
    unsafe {
        let mut h: HKEY = null_mut();
        let st = RegCreateKeyExW(
            root,
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
        let st = match data {
            RegData::Dword(d) => {
                let bytes = d.to_le_bytes();
                RegSetValueExW(h.0, v.as_ptr(), 0, REG_DWORD, bytes.as_ptr(), 4)
            }
            RegData::Text(t) => {
                let w = wide(t);
                RegSetValueExW(
                    h.0,
                    v.as_ptr(),
                    0,
                    REG_SZ,
                    w.as_ptr() as *const u8,
                    (w.len() * 2) as u32,
                )
            }
        };
        match st {
            0 => Ok(()),
            code => Err(win_err(value, code)),
        }
    }
}

pub(crate) fn reg_delete(key: &str, value: &str) -> Result<(), String> {
    let (root, sub) = split_hive(key)?;
    let k = wide(sub);
    let v = wide(value);
    // SAFETY: chaînes larges terminées par zéro.
    unsafe {
        let mut h: HKEY = null_mut();
        match RegOpenKeyExW(root, k.as_ptr(), 0, KEY_SET_VALUE, &mut h) {
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
