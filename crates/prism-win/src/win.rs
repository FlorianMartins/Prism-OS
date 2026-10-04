//! Implémentation Windows de `Platform` (windows-sys, aucun pilote).

use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::ptr::{null, null_mut};

use prism_core::journal::Undo;
use prism_core::mock::ALREADY_LOWER;
use prism_core::model::{EcoState, MemPriority, MemStatus, Priority, ProcId, ProcInfo, PurgeScope, Snapshot, Target};
use prism_core::plan::Action;
use prism_core::platform::{Outcome, Platform};

use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, LocalFree, ERROR_ACCESS_DENIED, ERROR_INVALID_PARAMETER, FILETIME, HANDLE,
    INVALID_HANDLE_VALUE, LUID,
};
use windows_sys::Win32::Security::{
    AdjustTokenPrivileges, LookupPrivilegeValueW, LUID_AND_ATTRIBUTES, SE_PRIVILEGE_ENABLED, TOKEN_ADJUST_PRIVILEGES,
    TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Power::{PowerGetActiveScheme, PowerSetActiveScheme};
use windows_sys::Win32::System::ProcessStatus::{K32EmptyWorkingSet, K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, GetPriorityClass, GetProcessInformation, GetProcessTimes, OpenProcess,
    OpenProcessToken, ProcessMemoryPriority, ProcessPowerThrottling, QueryFullProcessImageNameW, SetPriorityClass,
    SetProcessInformation, ABOVE_NORMAL_PRIORITY_CLASS, BELOW_NORMAL_PRIORITY_CLASS, HIGH_PRIORITY_CLASS,
    IDLE_PRIORITY_CLASS, MEMORY_PRIORITY_INFORMATION, NORMAL_PRIORITY_CLASS, PROCESS_ACCESS_RIGHTS, PROCESS_NAME_WIN32,
    PROCESS_POWER_THROTTLING_CURRENT_VERSION, PROCESS_POWER_THROTTLING_EXECUTION_SPEED, PROCESS_POWER_THROTTLING_STATE,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_INFORMATION, PROCESS_SET_QUOTA, REALTIME_PRIORITY_CLASS,
};

use crate::{format_guid, parse_guid};

/// Plan « Performances élevées » (GUID_MIN_POWER_SAVINGS).
const HIGH_PERFORMANCE: GUID = GUID::from_u128(0x8c5e7fda_e8bf_4a96_9a85_a6e23a8c635c);
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

// Informations système non exposées par windows-sys : déclarations documentées par
// les en-têtes du WDK / phnt (SYSTEM_INFORMATION_CLASS::SystemMemoryListInformation).
const SYSTEM_MEMORY_LIST_INFORMATION: i32 = 80;
const MEMORY_PURGE_STANDBY_LIST: u32 = 4;
const MEMORY_PURGE_LOW_PRIORITY_STANDBY_LIST: u32 = 5;
const STATUS_PRIVILEGE_NOT_HELD: i32 = 0xC000_0061_u32 as i32;

#[repr(C)]
#[derive(Default)]
struct SystemMemoryListInformation {
    zero_page_count: usize,
    free_page_count: usize,
    modified_page_count: usize,
    modified_no_write_page_count: usize,
    bad_page_count: usize,
    page_count_by_priority: [usize; 8],
    repurposed_pages_by_priority: [usize; 8],
    modified_page_count_page_file: usize,
}

#[link(name = "ntdll")]
extern "system" {
    fn NtQuerySystemInformation(class: i32, info: *mut c_void, len: u32, ret_len: *mut u32) -> i32;
    fn NtSetSystemInformation(class: i32, info: *mut c_void, len: u32) -> i32;
}

const PAGE: u64 = 4096;

/// Handle fermé automatiquement.
struct Owned(HANDLE);

impl Owned {
    fn open(pid: u32, access: PROCESS_ACCESS_RIGHTS) -> Result<Owned, u32> {
        // SAFETY: appel FFI sans pointeur ; le handle rendu est vérifié avant usage.
        let h = unsafe { OpenProcess(access, 0, pid) };
        if h.is_null() {
            Err(unsafe { GetLastError() })
        } else {
            Ok(Owned(h))
        }
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: handle valide, possédé par cette structure, fermé une seule fois.
        unsafe { CloseHandle(self.0) };
    }
}

fn filetime_u64(ft: FILETIME) -> u64 {
    ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64
}

fn creation_time(h: HANDLE) -> Option<u64> {
    // SAFETY: les quatre FILETIME sont des sorties locales correctement dimensionnées.
    unsafe {
        let (mut c, mut e, mut k, mut u): (FILETIME, FILETIME, FILETIME, FILETIME) =
            (zeroed(), zeroed(), zeroed(), zeroed());
        (GetProcessTimes(h, &mut c, &mut e, &mut k, &mut u) != 0).then(|| filetime_u64(c))
    }
}

fn image_path(h: HANDLE) -> Option<String> {
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    // SAFETY: `len` donne la capacité de `buf` ; Windows écrit au plus `len` caractères.
    let ok = unsafe { QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len) };
    (ok != 0).then(|| String::from_utf16_lossy(&buf[..len as usize]).to_lowercase())
}

fn working_set(h: HANDLE) -> u64 {
    // SAFETY: structure de sortie locale, taille passée explicitement.
    unsafe {
        let mut pmc: PROCESS_MEMORY_COUNTERS = zeroed();
        pmc.cb = size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        if K32GetProcessMemoryInfo(h, &mut pmc, pmc.cb) != 0 {
            pmc.WorkingSetSize as u64
        } else {
            0
        }
    }
}

fn session_of(pid: u32) -> u32 {
    let mut s = u32::MAX;
    // SAFETY: sortie locale.
    if unsafe { ProcessIdToSessionId(pid, &mut s) } == 0 {
        u32::MAX
    } else {
        s
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Active un privilège déjà détenu par le jeton (aucune élévation n'est créée).
fn enable_privilege(name: &str) -> bool {
    // SAFETY: jeton du processus courant, structures locales, handle fermé par Owned.
    unsafe {
        let mut token: HANDLE = null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let token = Owned(token);
        let mut luid: LUID = zeroed();
        let name = wide(name);
        if LookupPrivilegeValueW(null(), name.as_ptr(), &mut luid) == 0 {
            return false;
        }
        let tp = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES {
                Luid: luid,
                Attributes: SE_PRIVILEGE_ENABLED,
            }],
        };
        AdjustTokenPrivileges(token.0, 0, &tp, 0, null_mut(), null_mut()) != 0 && GetLastError() == 0
    }
}

fn priority_from_class(c: u32) -> Option<Priority> {
    Some(match c {
        IDLE_PRIORITY_CLASS => Priority::Idle,
        BELOW_NORMAL_PRIORITY_CLASS => Priority::BelowNormal,
        NORMAL_PRIORITY_CLASS => Priority::Normal,
        ABOVE_NORMAL_PRIORITY_CLASS => Priority::AboveNormal,
        HIGH_PRIORITY_CLASS => Priority::High,
        REALTIME_PRIORITY_CLASS => Priority::Realtime,
        _ => return None,
    })
}

fn class_from_priority(p: Priority) -> u32 {
    match p {
        Priority::Idle => IDLE_PRIORITY_CLASS,
        Priority::BelowNormal => BELOW_NORMAL_PRIORITY_CLASS,
        Priority::Normal => NORMAL_PRIORITY_CLASS,
        Priority::AboveNormal => ABOVE_NORMAL_PRIORITY_CLASS,
        Priority::High => HIGH_PRIORITY_CLASS,
        Priority::Realtime => REALTIME_PRIORITY_CLASS,
    }
}

fn last_error_outcome(what: &str) -> Outcome {
    // SAFETY: lecture du code d'erreur du thread courant.
    let code = unsafe { GetLastError() };
    if code == ERROR_ACCESS_DENIED {
        Outcome::Skipped("accès refusé".into())
    } else {
        Outcome::Failed(format!("{what} (erreur Windows {code})"))
    }
}

/// Ouvre le processus visé et vérifie qu'il s'agit bien du même (PID + création).
fn open_target(target: &Target, access: PROCESS_ACCESS_RIGHTS) -> Result<Owned, Outcome> {
    let h = Owned::open(target.id.pid, access | PROCESS_QUERY_LIMITED_INFORMATION).map_err(|code| match code {
        ERROR_INVALID_PARAMETER => Outcome::Skipped("processus disparu".into()),
        ERROR_ACCESS_DENIED => Outcome::Skipped("accès refusé".into()),
        c => Outcome::Failed(format!("ouverture impossible (erreur Windows {c})")),
    })?;
    match creation_time(h.0) {
        Some(c) if c == target.id.created => Ok(h),
        Some(_) => Err(Outcome::Skipped("processus disparu (PID réutilisé)".into())),
        None => Err(Outcome::Skipped("identité illisible".into())),
    }
}

fn get_memory_priority(h: HANDLE) -> Option<MemPriority> {
    let mut info = MEMORY_PRIORITY_INFORMATION { MemoryPriority: 0 };
    // SAFETY: structure de sortie locale, taille exacte.
    let ok = unsafe {
        GetProcessInformation(
            h,
            ProcessMemoryPriority,
            &mut info as *mut _ as *mut c_void,
            size_of::<MEMORY_PRIORITY_INFORMATION>() as u32,
        )
    };
    (ok != 0).then(|| MemPriority::from_level(info.MemoryPriority))
}

fn set_memory_priority(h: HANDLE, p: MemPriority) -> bool {
    let info = MEMORY_PRIORITY_INFORMATION {
        MemoryPriority: p.level(),
    };
    // SAFETY: structure d'entrée locale, taille exacte.
    unsafe {
        SetProcessInformation(
            h,
            ProcessMemoryPriority,
            &info as *const _ as *const c_void,
            size_of::<MEMORY_PRIORITY_INFORMATION>() as u32,
        ) != 0
    }
}

fn get_eco(h: HANDLE) -> EcoState {
    let mut st = PROCESS_POWER_THROTTLING_STATE {
        Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
        ControlMask: 0,
        StateMask: 0,
    };
    // SAFETY: structure de sortie locale, taille exacte.
    let ok = unsafe {
        GetProcessInformation(
            h,
            ProcessPowerThrottling,
            &mut st as *mut _ as *mut c_void,
            size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
        )
    };
    // Lecture non prise en charge avant Windows 11 : on suppose le cas courant.
    if ok == 0 || st.ControlMask & PROCESS_POWER_THROTTLING_EXECUTION_SPEED == 0 {
        EcoState::SystemManaged
    } else if st.StateMask & PROCESS_POWER_THROTTLING_EXECUTION_SPEED != 0 {
        EcoState::On
    } else {
        EcoState::Off
    }
}

fn set_eco(h: HANDLE, state: EcoState) -> bool {
    let speed = PROCESS_POWER_THROTTLING_EXECUTION_SPEED;
    let (control, value) = match state {
        EcoState::SystemManaged => (0, 0),
        EcoState::On => (speed, speed),
        EcoState::Off => (speed, 0),
    };
    let st = PROCESS_POWER_THROTTLING_STATE {
        Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
        ControlMask: control,
        StateMask: value,
    };
    // SAFETY: structure d'entrée locale, taille exacte.
    unsafe {
        SetProcessInformation(
            h,
            ProcessPowerThrottling,
            &st as *const _ as *const c_void,
            size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
        ) != 0
    }
}

/// Plan d'alimentation actif, sous forme de GUID texte.
pub fn active_power_plan() -> Option<String> {
    let mut p: *mut GUID = null_mut();
    // SAFETY: Windows alloue le GUID ; on le copie puis on le libère avec LocalFree.
    unsafe {
        if PowerGetActiveScheme(null_mut(), &mut p) != 0 || p.is_null() {
            return None;
        }
        let g = *p;
        LocalFree(p as *mut c_void);
        Some(format_guid(g.data1, g.data2, g.data3, g.data4))
    }
}

fn set_power_plan(guid: &GUID) -> u32 {
    // SAFETY: GUID d'entrée local.
    unsafe { PowerSetActiveScheme(null_mut(), guid) }
}

/// Listes mémoire brutes de Windows, en octets (diagnostic, `memlab`).
#[derive(Clone, Copy, Debug, Default)]
pub struct MemoryLists {
    pub zero: u64,
    pub free: u64,
    pub modified: u64,
    pub standby_by_priority: [u64; 8],
}

/// Nécessite les droits administrateur (SeProfileSingleProcessPrivilege).
pub fn memory_lists() -> Option<MemoryLists> {
    let mut info = SystemMemoryListInformation::default();
    // SAFETY: structure de sortie locale, taille exacte.
    let st = unsafe {
        NtQuerySystemInformation(
            SYSTEM_MEMORY_LIST_INFORMATION,
            &mut info as *mut _ as *mut c_void,
            size_of::<SystemMemoryListInformation>() as u32,
            null_mut(),
        )
    };
    if st < 0 {
        return None;
    }
    let pages = |n: usize| n as u64 * PAGE;
    Some(MemoryLists {
        zero: pages(info.zero_page_count),
        free: pages(info.free_page_count),
        modified: pages(info.modified_page_count),
        standby_by_priority: info.page_count_by_priority.map(pages),
    })
}

fn memory_status() -> MemStatus {
    // SAFETY: structures de sortie locales, tailles passées explicitement.
    unsafe {
        let mut ms: MEMORYSTATUSEX = zeroed();
        ms.dwLength = size_of::<MEMORYSTATUSEX>() as u32;
        GlobalMemoryStatusEx(&mut ms);
        let mut info = SystemMemoryListInformation::default();
        let st = NtQuerySystemInformation(
            SYSTEM_MEMORY_LIST_INFORMATION,
            &mut info as *mut _ as *mut c_void,
            size_of::<SystemMemoryListInformation>() as u32,
            null_mut(),
        );
        if st >= 0 {
            let pages = |n: usize| n as u64 * PAGE;
            MemStatus {
                total: ms.ullTotalPhys,
                free: pages(info.free_page_count + info.zero_page_count),
                standby_low: pages(info.page_count_by_priority[..3].iter().sum()),
                standby_total: pages(info.page_count_by_priority.iter().sum()),
            }
        } else {
            // Sans droits administrateur : « disponible » inclut le cache, on le dit
            // tel quel plutôt que d'inventer une répartition.
            MemStatus {
                total: ms.ullTotalPhys,
                free: ms.ullAvailPhys,
                standby_low: 0,
                standby_total: 0,
            }
        }
    }
}

fn purge_standby(scope: PurgeScope) -> Outcome {
    let mut command = match scope {
        PurgeScope::Off => return Outcome::Done(None),
        PurgeScope::Low => MEMORY_PURGE_LOW_PRIORITY_STANDBY_LIST,
        PurgeScope::All => MEMORY_PURGE_STANDBY_LIST,
    };
    // SAFETY: commande u32 locale, taille exacte.
    let st =
        unsafe { NtSetSystemInformation(SYSTEM_MEMORY_LIST_INFORMATION, &mut command as *mut _ as *mut c_void, 4) };
    match st {
        s if s >= 0 => Outcome::Done(None),
        STATUS_PRIVILEGE_NOT_HELD => Outcome::Skipped("droits administrateur requis".into()),
        s => Outcome::Failed(format!("purge refusée (NTSTATUS {:#x})", s as u32)),
    }
}

/// État réel d'un processus, relu par l'API (diagnostic et tests).
pub fn process_state(target: &Target) -> Option<(Priority, EcoState, MemPriority)> {
    let h = open_target(target, PROCESS_QUERY_LIMITED_INFORMATION).ok()?;
    // SAFETY: handle vérifié.
    let prio = priority_from_class(unsafe { GetPriorityClass(h.0) })?;
    Some((prio, get_eco(h.0), get_memory_priority(h.0)?))
}

/// Identité complète d'un processus à partir de son PID (tests, diagnostic).
pub fn proc_id(pid: u32) -> Option<ProcId> {
    let h = Owned::open(pid, PROCESS_QUERY_LIMITED_INFORMATION).ok()?;
    Some(ProcId {
        pid,
        created: creation_time(h.0)?,
    })
}

pub struct WindowsPlatform {
    self_pid: u32,
    user_session: u32,
    /// Vrai si le privilège de purge du cache a pu être activé (administrateur).
    pub can_purge: bool,
}

impl WindowsPlatform {
    pub fn new() -> WindowsPlatform {
        // SAFETY: appel sans argument.
        let self_pid = unsafe { GetCurrentProcessId() };
        let can_purge = enable_privilege("SeProfileSingleProcessPrivilege");
        WindowsPlatform {
            self_pid,
            user_session: session_of(self_pid),
            can_purge,
        }
    }
}

impl Default for WindowsPlatform {
    fn default() -> Self {
        Self::new()
    }
}

impl Platform for WindowsPlatform {
    fn snapshot(&mut self) -> Result<Snapshot, String> {
        // SAFETY: instantané Toolhelp ; la structure est initialisée avec sa taille et
        // le handle est fermé par Owned.
        let procs = unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snap == INVALID_HANDLE_VALUE {
                return Err(format!(
                    "instantané des processus impossible (erreur {})",
                    GetLastError()
                ));
            }
            let snap = Owned(snap);
            let mut entry: PROCESSENTRY32W = zeroed();
            entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
            let mut procs = Vec::new();
            let mut more = Process32FirstW(snap.0, &mut entry) != 0;
            while more {
                let pid = entry.th32ProcessID;
                let len = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                let name = String::from_utf16_lossy(&entry.szExeFile[..len]).to_lowercase();
                let session = session_of(pid);
                let (created, path, ws) = match Owned::open(pid, PROCESS_QUERY_LIMITED_INFORMATION) {
                    Ok(h) => (creation_time(h.0).unwrap_or(0), image_path(h.0), working_set(h.0)),
                    Err(_) => (0, None, 0),
                };
                procs.push(ProcInfo {
                    id: ProcId { pid, created },
                    name,
                    path,
                    session,
                    working_set: ws,
                });
                more = Process32NextW(snap.0, &mut entry) != 0;
            }
            procs
        };
        Ok(Snapshot {
            procs,
            mem: memory_status(),
            user_session: self.user_session,
            self_pid: self.self_pid,
        })
    }

    fn apply(&mut self, action: &Action) -> Outcome {
        match action {
            Action::Priority { target, to } => {
                let h = match open_target(target, PROCESS_SET_INFORMATION) {
                    Ok(h) => h,
                    Err(o) => return o,
                };
                // SAFETY: handle vérifié.
                let current = unsafe { GetPriorityClass(h.0) };
                let Some(previous) = priority_from_class(current) else {
                    return Outcome::Skipped(format!("priorité inconnue {current:#x}"));
                };
                if previous.rank() <= to.rank() {
                    return Outcome::Skipped(ALREADY_LOWER.into());
                }
                // SAFETY: handle vérifié, classe de priorité valide.
                if unsafe { SetPriorityClass(h.0, class_from_priority(*to)) } == 0 {
                    return last_error_outcome("SetPriorityClass");
                }
                Outcome::Done(Some(Undo::Priority {
                    target: target.clone(),
                    previous,
                }))
            }
            Action::EcoQos { target } => {
                let h = match open_target(target, PROCESS_SET_INFORMATION) {
                    Ok(h) => h,
                    Err(o) => return o,
                };
                let previous = get_eco(h.0);
                if previous == EcoState::On {
                    return Outcome::Skipped(ALREADY_LOWER.into());
                }
                if !set_eco(h.0, EcoState::On) {
                    return last_error_outcome("EcoQoS");
                }
                Outcome::Done(Some(Undo::EcoQos {
                    target: target.clone(),
                    previous,
                }))
            }
            Action::MemoryPriority { target, to } => {
                let h = match open_target(target, PROCESS_SET_INFORMATION) {
                    Ok(h) => h,
                    Err(o) => return o,
                };
                let Some(previous) = get_memory_priority(h.0) else {
                    return last_error_outcome("lecture de la priorité mémoire");
                };
                if previous <= *to {
                    return Outcome::Skipped(ALREADY_LOWER.into());
                }
                if !set_memory_priority(h.0, *to) {
                    return last_error_outcome("priorité mémoire");
                }
                Outcome::Done(Some(Undo::MemoryPriority {
                    target: target.clone(),
                    previous,
                }))
            }
            Action::TrimWorkingSet { target } => {
                let h = match open_target(target, PROCESS_SET_QUOTA) {
                    Ok(h) => h,
                    Err(o) => return o,
                };
                // SAFETY: handle vérifié avec PROCESS_SET_QUOTA.
                if unsafe { K32EmptyWorkingSet(h.0) } == 0 {
                    return last_error_outcome("EmptyWorkingSet");
                }
                Outcome::Done(None)
            }
            Action::PurgeStandby { scope } => purge_standby(*scope),
            Action::HighPerformancePower => {
                let Some(previous) = active_power_plan() else {
                    return Outcome::Failed("plan d'alimentation actif illisible".into());
                };
                if previous
                    == format_guid(
                        HIGH_PERFORMANCE.data1,
                        HIGH_PERFORMANCE.data2,
                        HIGH_PERFORMANCE.data3,
                        HIGH_PERFORMANCE.data4,
                    )
                {
                    return Outcome::Skipped("déjà en performances élevées".into());
                }
                match set_power_plan(&HIGH_PERFORMANCE) {
                    0 => Outcome::Done(Some(Undo::PowerPlan { previous })),
                    code => Outcome::Skipped(format!("plan performances élevées indisponible (erreur {code})")),
                }
            }
            Action::ShutdownWsl => match Command::new("wsl.exe")
                .arg("--shutdown")
                .creation_flags(CREATE_NO_WINDOW)
                .status()
            {
                Ok(s) if s.success() => Outcome::Done(None),
                Ok(s) => Outcome::Failed(format!("wsl --shutdown a échoué ({s})")),
                Err(e) => Outcome::Skipped(format!("wsl.exe introuvable ({e})")),
            },
        }
    }

    fn undo(&mut self, undo: &Undo) -> Outcome {
        match undo {
            Undo::Priority { target, previous } => match open_target(target, PROCESS_SET_INFORMATION) {
                Err(o) => o,
                // SAFETY: handle vérifié, classe valide.
                Ok(h) if unsafe { SetPriorityClass(h.0, class_from_priority(*previous)) } != 0 => Outcome::Done(None),
                Ok(_) => last_error_outcome("SetPriorityClass"),
            },
            Undo::EcoQos { target, previous } => match open_target(target, PROCESS_SET_INFORMATION) {
                Err(o) => o,
                Ok(h) if set_eco(h.0, *previous) => Outcome::Done(None),
                Ok(_) => last_error_outcome("EcoQoS"),
            },
            Undo::MemoryPriority { target, previous } => match open_target(target, PROCESS_SET_INFORMATION) {
                Err(o) => o,
                Ok(h) if set_memory_priority(h.0, *previous) => Outcome::Done(None),
                Ok(_) => last_error_outcome("priorité mémoire"),
            },
            Undo::PowerPlan { previous } => {
                let Some((d1, d2, d3, d4)) = parse_guid(previous) else {
                    return Outcome::Failed(format!("GUID de plan invalide dans le journal : {previous}"));
                };
                let guid = GUID {
                    data1: d1,
                    data2: d2,
                    data3: d3,
                    data4: d4,
                };
                match set_power_plan(&guid) {
                    0 => Outcome::Done(None),
                    code => Outcome::Failed(format!("restauration du plan impossible (erreur {code})")),
                }
            }
        }
    }
}
