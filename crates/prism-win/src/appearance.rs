//! Apparence : SystemParametersInfo (animations, effets) et registre (thème,
//! réactivité). Réglages officiels uniquement, aucune injection.

use std::ffi::c_void;
use std::ptr::null_mut;

use prism_core::apparence::{AppearanceConfig, KnobValue, RegSetting, Setting, SpiSetting, APPEARANCE_ALLOWLIST};
use windows_sys::Win32::Foundation::{LPARAM, WPARAM};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    SendMessageTimeoutW, SystemParametersInfoW, ANIMATIONINFO, HWND_BROADCAST, SMTO_ABORTIFHUNG, SPIF_SENDCHANGE,
    SPIF_UPDATEINIFILE, SPI_GETANIMATION, SPI_SETANIMATION, WM_SETTINGCHANGE,
};

use crate::sysconfig::{reg_delete, reg_get, reg_set};

const PERSIST: u32 = SPIF_UPDATEINIFILE | SPIF_SENDCHANGE;

fn allowed(full_key: &str) -> bool {
    let k = full_key.to_ascii_lowercase();
    APPEARANCE_ALLOWLIST.iter().any(|a| k == *a)
}

fn last_error(what: &str) -> String {
    format!("{what} : erreur Windows {}", std::io::Error::last_os_error())
}

pub struct WindowsAppearance;

impl AppearanceConfig for WindowsAppearance {
    fn read(&mut self, s: &Setting) -> Result<Option<KnobValue>, String> {
        match s {
            Setting::MinMax(_) => {
                let mut ai = ANIMATIONINFO {
                    cbSize: size_of::<ANIMATIONINFO>() as u32,
                    iMinAnimate: 0,
                };
                // SAFETY: structure de sortie locale, taille annoncée.
                let ok =
                    unsafe { SystemParametersInfoW(SPI_GETANIMATION, ai.cbSize, &mut ai as *mut _ as *mut c_void, 0) };
                if ok == 0 {
                    return Err(last_error("SPI_GETANIMATION"));
                }
                Ok(Some(KnobValue::Bool(ai.iMinAnimate != 0)))
            }
            Setting::Spi(SpiSetting { spi }) => {
                let mut v: i32 = 0;
                // SAFETY: les SPI_GET* booléens écrivent un BOOL dans pvParam.
                let ok = unsafe { SystemParametersInfoW(spi.get, 0, &mut v as *mut _ as *mut c_void, 0) };
                if ok == 0 {
                    return Err(last_error("SystemParametersInfo"));
                }
                Ok(Some(KnobValue::Bool(v != 0)))
            }
            Setting::Reg(RegSetting { reg }) => {
                Ok(reg_get(&reg.full_key(), &reg.value)?.as_ref().map(KnobValue::from_reg))
            }
        }
    }

    fn write(&mut self, s: &Setting, v: Option<&KnobValue>) -> Result<(), String> {
        match (s, v) {
            (Setting::MinMax(_), Some(KnobValue::Bool(b))) => {
                let mut ai = ANIMATIONINFO {
                    cbSize: size_of::<ANIMATIONINFO>() as u32,
                    iMinAnimate: *b as i32,
                };
                // SAFETY: structure d'entrée locale, taille annoncée.
                let ok = unsafe {
                    SystemParametersInfoW(SPI_SETANIMATION, ai.cbSize, &mut ai as *mut _ as *mut c_void, PERSIST)
                };
                if ok == 0 {
                    return Err(last_error("SPI_SETANIMATION"));
                }
                Ok(())
            }
            (Setting::Spi(SpiSetting { spi }), Some(KnobValue::Bool(b))) => {
                // SAFETY: les SPI_SET* booléens prennent la valeur dans uiParam ou,
                // pour la plupart, directement dans pvParam (castée en pointeur).
                let ok = unsafe {
                    if spi.ui {
                        SystemParametersInfoW(spi.set, *b as u32, null_mut(), PERSIST)
                    } else {
                        SystemParametersInfoW(spi.set, 0, *b as usize as *mut c_void, PERSIST)
                    }
                };
                if ok == 0 {
                    return Err(last_error("SystemParametersInfo"));
                }
                Ok(())
            }
            (Setting::Reg(RegSetting { reg }), v) => {
                let key = reg.full_key();
                if !allowed(&key) {
                    return Err(format!("{key} : clé hors de la liste autorisée"));
                }
                match v.and_then(KnobValue::to_reg) {
                    Some(d) => reg_set(&key, &reg.value, &d),
                    None if v.is_none() => reg_delete(&key, &reg.value),
                    None => Err("valeur booléenne pour un réglage de registre".into()),
                }
            }
            (_, None) => Ok(()),
            _ => Err("type de valeur inattendu".into()),
        }
    }

    fn notify(&mut self) {
        // L'Explorateur relit le thème, la transparence et la barre des tâches.
        for area in ["ImmersiveColorSet", "WindowsThemeElement", "TraySettings"] {
            let w: Vec<u16> = area.encode_utf16().chain(Some(0)).collect();
            // SAFETY: chaîne large terminée par zéro, valable pendant l'appel.
            unsafe {
                SendMessageTimeoutW(
                    HWND_BROADCAST,
                    WM_SETTINGCHANGE,
                    0 as WPARAM,
                    w.as_ptr() as LPARAM,
                    SMTO_ABORTIFHUNG,
                    1000,
                    null_mut(),
                );
            }
        }
    }
}
