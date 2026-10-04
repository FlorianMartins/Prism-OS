//! Collecte pour `prism rapport` (`prism_core::rapport`) : compteurs de mémoire,
//! mémoire de chaque processus (toutes sessions), services par hôte `svchost`.

use std::collections::HashMap;
use std::mem::{size_of, zeroed};
use std::ptr::null_mut;

use prism_core::allege::RegData;
use prism_core::rapport::{Hote, Memoire, Processus};
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_MORE_DATA, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::ProcessStatus::{
    K32GetPerformanceInfo, K32GetProcessMemoryInfo, PERFORMANCE_INFORMATION, PROCESS_MEMORY_COUNTERS,
    PROCESS_MEMORY_COUNTERS_EX,
};
use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows_sys::Win32::System::Services::{
    EnumServicesStatusExW, OpenSCManagerW, ENUM_SERVICE_STATUS_PROCESSW, SC_ENUM_PROCESS_INFO,
    SC_MANAGER_ENUMERATE_SERVICE, SERVICE_ACTIVE, SERVICE_WIN32,
};
use windows_sys::Win32::System::SystemInformation::{GetPhysicallyInstalledSystemMemory, GetTickCount64};
use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

use crate::sysconfig::{reg_get, Sc};

fn text(key: &str, value: &str) -> Option<String> {
    match reg_get(key, value) {
        Ok(Some(RegData::Text(t))) => Some(t),
        Ok(Some(RegData::Dword(d))) => Some(d.to_string()),
        _ => None,
    }
}

/// « Windows 11 Pro 25H2 (build 26200) ». Le registre dit encore « Windows 10 » sur
/// Windows 11 : corrigé d'après le numéro de build.
pub fn windows() -> String {
    const NT: &str = "HKLM\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion";
    let name = text(NT, "ProductName").unwrap_or_else(|| "Windows".into());
    let build = text(NT, "CurrentBuild").unwrap_or_default();
    let name = if build.parse::<u32>().is_ok_and(|b| b >= 22000) {
        name.replace("Windows 10", "Windows 11")
    } else {
        name
    };
    let version = text(NT, "DisplayVersion").unwrap_or_default();
    format!("{name} {version} (build {build})")
}

pub fn processeur() -> String {
    let name = text(
        "HKLM\\HARDWARE\\DESCRIPTION\\System\\CentralProcessor\\0",
        "ProcessorNameString",
    )
    .unwrap_or_else(|| "processeur inconnu".into());
    format!(
        "{} · {} cœurs logiques",
        name.trim(),
        std::thread::available_parallelism().map_or(0, |n| n.get())
    )
}

pub fn allume_depuis_secs() -> u64 {
    // SAFETY: sans paramètre.
    unsafe { GetTickCount64() / 1000 }
}

pub fn memoire(compressee: u64) -> Memoire {
    let mut m = Memoire {
        compressee,
        ..Default::default()
    };
    // SAFETY: structures de sortie locales, tailles annoncées.
    unsafe {
        let mut kb = 0u64;
        if GetPhysicallyInstalledSystemMemory(&mut kb) != 0 {
            m.installee = kb * 1024;
        }
        let mut pi: PERFORMANCE_INFORMATION = zeroed();
        pi.cb = size_of::<PERFORMANCE_INFORMATION>() as u32;
        if K32GetPerformanceInfo(&mut pi, pi.cb) != 0 {
            let page = pi.PageSize as u64;
            m.totale = pi.PhysicalTotal as u64 * page;
            m.disponible = pi.PhysicalAvailable as u64 * page;
            m.engagee = pi.CommitTotal as u64 * page;
            m.limite_engagee = pi.CommitLimit as u64 * page;
            m.pool_pagine = pi.KernelPaged as u64 * page;
            m.pool_non_pagine = pi.KernelNonpaged as u64 * page;
        }
    }
    if let Some(l) = crate::win::memory_lists() {
        m.cache = l.standby_by_priority.iter().sum();
        m.modifiee = l.modified;
        m.libre = l.free + l.zero;
    }
    m
}

/// Tous les processus (toutes sessions) avec leur mémoire, et la mémoire de travail
/// du processus « Memory Compression » (la mémoire compressée).
pub fn processus() -> (Vec<Processus>, HashMap<u32, (u64, u64)>, u64) {
    let mut out = Vec::new();
    let mut by_pid = HashMap::new();
    let mut compressee = 0;
    // SAFETY: instantané Toolhelp fermé à la fin ; chaque handle de processus est vérifié
    // puis fermé ; structures locales de taille annoncée.
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return (out, by_pid, 0);
        }
        let mut e: PROCESSENTRY32W = zeroed();
        e.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        let mut more = Process32FirstW(snap, &mut e) != 0;
        while more {
            let len = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
            let nom = String::from_utf16_lossy(&e.szExeFile[..len]);
            let pid = e.th32ProcessID;
            let mut session = 0u32;
            ProcessIdToSessionId(pid, &mut session);
            let (mut prive, mut ws) = (0u64, 0u64);
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if !h.is_null() {
                let mut c: PROCESS_MEMORY_COUNTERS_EX = zeroed();
                c.cb = size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
                if K32GetProcessMemoryInfo(h, &mut c as *mut _ as *mut PROCESS_MEMORY_COUNTERS, c.cb) != 0 {
                    prive = c.PrivateUsage as u64;
                    ws = c.WorkingSetSize as u64;
                }
                CloseHandle(h);
            }
            if nom.eq_ignore_ascii_case("Memory Compression") {
                compressee = ws;
            }
            if pid > 0 {
                by_pid.insert(pid, (ws, prive));
                out.push(Processus {
                    nom,
                    session,
                    prive,
                    ws,
                });
            }
            more = Process32NextW(snap, &mut e) != 0;
        }
        CloseHandle(snap);
    }
    (out, by_pid, compressee)
}

/// Services en marche regroupés par processus hôte.
pub fn hotes(by_pid: &HashMap<u32, (u64, u64)>) -> Vec<Hote> {
    let mut map: HashMap<u32, Vec<String>> = HashMap::new();
    // SAFETY: premier appel pour la taille, second avec un tampon aligné assez grand ;
    // le tampon contient des structures suivies de leurs chaînes, lues dans ses limites.
    unsafe {
        let scm = OpenSCManagerW(std::ptr::null(), std::ptr::null(), SC_MANAGER_ENUMERATE_SERVICE);
        if scm.is_null() {
            return Vec::new();
        }
        let scm = Sc(scm);
        let (mut needed, mut count, mut resume) = (0u32, 0u32, 0u32);
        EnumServicesStatusExW(
            scm.0,
            SC_ENUM_PROCESS_INFO,
            SERVICE_WIN32,
            SERVICE_ACTIVE,
            null_mut(),
            0,
            &mut needed,
            &mut count,
            &mut resume,
            std::ptr::null(),
        );
        if GetLastError() != ERROR_MORE_DATA || needed == 0 {
            return Vec::new();
        }
        let mut buf: Vec<u64> = vec![0; (needed as usize).div_ceil(8) + 1];
        resume = 0;
        if EnumServicesStatusExW(
            scm.0,
            SC_ENUM_PROCESS_INFO,
            SERVICE_WIN32,
            SERVICE_ACTIVE,
            buf.as_mut_ptr() as *mut u8,
            (buf.len() * 8) as u32,
            &mut needed,
            &mut count,
            &mut resume,
            std::ptr::null(),
        ) == 0
        {
            return Vec::new();
        }
        let list = std::slice::from_raw_parts(buf.as_ptr() as *const ENUM_SERVICE_STATUS_PROCESSW, count as usize);
        for s in list {
            let p = s.lpServiceName;
            let len = (0..).take_while(|i| *p.add(*i) != 0).count();
            let name = String::from_utf16_lossy(std::slice::from_raw_parts(p, len));
            map.entry(s.ServiceStatusProcess.dwProcessId).or_default().push(name);
        }
    }
    map.into_iter()
        .filter(|(pid, _)| *pid != 0)
        .map(|(pid, mut services)| {
            services.sort_by_key(|s| s.to_lowercase());
            let (ws, prive) = by_pid.get(&pid).copied().unwrap_or((0, 0));
            Hote { ws, prive, services }
        })
        .collect()
}
