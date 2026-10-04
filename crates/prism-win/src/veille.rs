//! Mise en veille des outils cyber pendant une partie protégée par un anti-cheat noyau
//! (`prism_core::noyau`) : services et pilotes, fermeture vérifiée, WSL.

use prism_core::model::ProcId;
use prism_core::noyau::Veille;
use windows_sys::Win32::Foundation::GetLastError;
use windows_sys::Win32::System::Services::{
    ControlService, QueryServiceStatus, StartServiceW, SERVICE_CONTROL_STOP, SERVICE_QUERY_STATUS, SERVICE_RUNNING,
    SERVICE_START, SERVICE_STATUS, SERVICE_STOP,
};

use crate::sysconfig::{open_service, win_err};

pub struct WindowsVeille;

impl Veille for WindowsVeille {
    fn service_en_marche(&mut self, name: &str) -> Result<Option<bool>, String> {
        let Some((svc, _scm)) = open_service(name, SERVICE_QUERY_STATUS)? else {
            return Ok(None);
        };
        // SAFETY: handle vérifié, sortie locale.
        unsafe {
            let mut st: SERVICE_STATUS = std::mem::zeroed();
            if QueryServiceStatus(svc.0, &mut st) == 0 {
                return Err(win_err(name, GetLastError()));
            }
            Ok(Some(st.dwCurrentState == SERVICE_RUNNING))
        }
    }

    fn arreter_service(&mut self, name: &str) -> Result<(), String> {
        let Some((svc, _scm)) = open_service(name, SERVICE_STOP | SERVICE_QUERY_STATUS)? else {
            return Err("service absent".into());
        };
        // SAFETY: handle vérifié, sortie locale.
        unsafe {
            let mut st: SERVICE_STATUS = std::mem::zeroed();
            if ControlService(svc.0, SERVICE_CONTROL_STOP, &mut st) == 0 {
                return Err(win_err(name, GetLastError()));
            }
        }
        Ok(())
    }

    fn demarrer_service(&mut self, name: &str) -> Result<(), String> {
        let Some((svc, _scm)) = open_service(name, SERVICE_START)? else {
            return Err("service absent".into());
        };
        // SAFETY: handle vérifié, sans arguments.
        unsafe {
            if StartServiceW(svc.0, 0, std::ptr::null()) == 0 {
                return Err(win_err(name, GetLastError()));
            }
        }
        Ok(())
    }

    fn fermer(&mut self, id: &ProcId) -> Result<(), String> {
        crate::win::terminate_verified(id)
    }

    fn eteindre_wsl(&mut self) -> Result<(), String> {
        use std::os::windows::process::CommandExt;
        let out = std::process::Command::new("wsl.exe")
            .arg("--shutdown")
            .creation_flags(0x0800_0000)
            .stdin(std::process::Stdio::null())
            .output()
            .map_err(|e| format!("wsl.exe : {e}"))?;
        if out.status.success() {
            Ok(())
        } else {
            Err(format!("wsl --shutdown : code {:?}", out.status.code()))
        }
    }
}
