//! Liste de tous les services Windows (page Services de l'appli).

use std::ptr::{null, null_mut};

use prism_core::allege::ServiceInfo;
use windows_sys::Win32::Foundation::{GetLastError, ERROR_MORE_DATA};
use windows_sys::Win32::System::Services::{
    EnumServicesStatusExW, OpenSCManagerW, ENUM_SERVICE_STATUS_PROCESSW, SC_ENUM_PROCESS_INFO,
    SC_MANAGER_ENUMERATE_SERVICE, SERVICE_QUERY_CONFIG, SERVICE_RUNNING, SERVICE_STATE_ALL, SERVICE_WIN32,
};

use crate::sysconfig::{open_service, read_start, Sc};

/// Bit « service par utilisateur » du type de service.
const SERVICE_USER_SERVICE: u32 = 0x40;

fn wstr(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    // SAFETY: chaîne terminée par zéro dans le tampon rendu par Windows.
    unsafe {
        let len = (0..).take_while(|i| *p.add(*i) != 0).count();
        String::from_utf16_lossy(std::slice::from_raw_parts(p, len))
    }
}

pub fn services_list() -> Result<Vec<ServiceInfo>, String> {
    let mut out = Vec::new();
    // SAFETY: premier appel pour la taille, second avec un tampon aligné assez grand ;
    // les chaînes pointent dans ce tampon, lues avant sa libération.
    unsafe {
        let scm = OpenSCManagerW(null(), null(), SC_MANAGER_ENUMERATE_SERVICE);
        if scm.is_null() {
            return Err(format!("gestionnaire de services (erreur {})", GetLastError()));
        }
        let scm = Sc(scm);
        let (mut needed, mut count, mut resume) = (0u32, 0u32, 0u32);
        EnumServicesStatusExW(
            scm.0,
            SC_ENUM_PROCESS_INFO,
            SERVICE_WIN32,
            SERVICE_STATE_ALL,
            null_mut(),
            0,
            &mut needed,
            &mut count,
            &mut resume,
            null(),
        );
        if GetLastError() != ERROR_MORE_DATA {
            return Err(format!("liste des services (erreur {})", GetLastError()));
        }
        let mut buf: Vec<u64> = vec![0; (needed as usize).div_ceil(8) + 1];
        resume = 0;
        if EnumServicesStatusExW(
            scm.0,
            SC_ENUM_PROCESS_INFO,
            SERVICE_WIN32,
            SERVICE_STATE_ALL,
            buf.as_mut_ptr() as *mut u8,
            (buf.len() * 8) as u32,
            &mut needed,
            &mut count,
            &mut resume,
            null(),
        ) == 0
        {
            return Err(format!("liste des services (erreur {})", GetLastError()));
        }
        let list = std::slice::from_raw_parts(buf.as_ptr() as *const ENUM_SERVICE_STATUS_PROCESSW, count as usize);
        for s in list {
            let name = wstr(s.lpServiceName);
            let start = open_service(&name, SERVICE_QUERY_CONFIG)
                .ok()
                .flatten()
                .and_then(|(svc, _scm)| read_start(&svc).ok());
            out.push(ServiceInfo {
                display: wstr(s.lpDisplayName),
                start,
                running: s.ServiceStatusProcess.dwCurrentState == SERVICE_RUNNING,
                per_user: s.ServiceStatusProcess.dwServiceType & SERVICE_USER_SERVICE != 0,
                name,
            });
        }
    }
    out.sort_by_key(|s| s.display.to_lowercase());
    Ok(out)
}
