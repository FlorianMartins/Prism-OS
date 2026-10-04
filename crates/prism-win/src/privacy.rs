//! Vie privée sous Windows : registre (liste autorisée de `prism_core::privacy`),
//! règles du Pare-feu Windows (`netsh advfirewall`, sortie non lue : seul le code de
//! retour compte, il ne dépend pas de la langue) et connexions réseau ouvertes par
//! les composants de télémétrie (`GetExtendedTcpTable`, comme `netstat -o`).

use std::ptr::null_mut;

use prism_core::allege::{RegData, StartType, SystemConfig};
use prism_core::privacy::{FwTarget, PrivacySystem, TelemetryConnection, RULE_PREFIX};

use windows_sys::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, MIB_TCPTABLE_OWNER_PID, TCP_TABLE_OWNER_PID_ALL,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};

use crate::sysconfig::{reg_delete, reg_get, reg_set, WindowsSystemConfig};

fn netsh(args: &[&str]) -> Result<std::process::Output, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    std::process::Command::new("netsh.exe")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("netsh.exe : {e}"))
}

/// `%SystemRoot%\…` -> chemin réel.
fn expand(path: &str) -> String {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
    let lower = path.to_ascii_lowercase();
    match lower.strip_prefix("%systemroot%") {
        Some(_) => format!("{root}{}", &path["%SystemRoot%".len()..]),
        None => path.to_string(),
    }
}

pub struct WindowsPrivacy {
    sys: WindowsSystemConfig,
}

impl WindowsPrivacy {
    pub fn new() -> WindowsPrivacy {
        WindowsPrivacy {
            sys: WindowsSystemConfig,
        }
    }
}

impl Default for WindowsPrivacy {
    fn default() -> Self {
        WindowsPrivacy::new()
    }
}

impl PrivacySystem for WindowsPrivacy {
    fn reg(&mut self, key: &str, value: &str) -> Result<Option<RegData>, String> {
        reg_get(key, value)
    }

    fn set_reg(&mut self, key: &str, value: &str, data: &RegData) -> Result<(), String> {
        // Dernière barrière : hors liste autorisée, rien n'est écrit.
        if !prism_core::privacy::registry_allowed(key) {
            return Err(format!("{key} : clé hors de la liste autorisée"));
        }
        reg_set(key, value, data)
    }

    fn delete_reg(&mut self, key: &str, value: &str) -> Result<(), String> {
        if !prism_core::privacy::registry_allowed(key) {
            return Err(format!("{key} : clé hors de la liste autorisée"));
        }
        reg_delete(key, value)
    }

    fn rule_exists(&mut self, name: &str) -> Result<bool, String> {
        let out = netsh(&["advfirewall", "firewall", "show", "rule", &format!("name={name}")])?;
        Ok(out.status.success())
    }

    fn add_rule(&mut self, name: &str, target: &FwTarget) -> Result<(), String> {
        if !name.starts_with(RULE_PREFIX) {
            return Err(format!("{name} : seules les règles de Prism sont créées"));
        }
        let what = match target {
            FwTarget::Service(s) => format!("service={s}"),
            FwTarget::Program(p) => {
                if !prism_core::privacy::program_allowed(p) {
                    return Err(format!("{p} : programme hors de Windows"));
                }
                format!("program={p}")
            }
        };
        let out = netsh(&[
            "advfirewall",
            "firewall",
            "add",
            "rule",
            &format!("name={name}"),
            "dir=out",
            "action=block",
            "enable=yes",
            "profile=any",
            &what,
        ])?;
        if out.status.success() {
            Ok(())
        } else {
            Err(format!("{name} : le pare-feu a refusé (droits administrateur ?)"))
        }
    }

    fn delete_rule(&mut self, name: &str) -> Result<(), String> {
        if !name.starts_with(RULE_PREFIX) {
            return Err(format!("{name} : seules les règles de Prism sont supprimées"));
        }
        if !self.rule_exists(name)? {
            return Ok(()); // déjà supprimée (à la main, ou par un autre outil)
        }
        let out = netsh(&["advfirewall", "firewall", "delete", "rule", &format!("name={name}")])?;
        if out.status.success() {
            Ok(())
        } else {
            Err(format!("{name} : suppression refusée (droits administrateur ?)"))
        }
    }

    fn target_exists(&mut self, target: &FwTarget) -> bool {
        match target {
            FwTarget::Service(s) => matches!(self.sys.service_start(s), Ok(Some(_))),
            FwTarget::Program(p) => std::path::Path::new(&expand(p)).exists(),
        }
    }

    fn service_start(&mut self, name: &str) -> Result<Option<StartType>, String> {
        self.sys.service_start(name)
    }

    fn task_enabled(&mut self, path: &str) -> Result<Option<bool>, String> {
        self.sys.task_enabled(path)
    }
}

/// Processus d'un composant : PID du service (s'il tourne) ou PID des programmes de
/// ce nom.
fn component_pids(catalog: &prism_core::privacy::Catalog) -> Vec<(u32, String)> {
    let mut out = Vec::new();
    let programs: Vec<String> = catalog
        .firewall
        .iter()
        .filter_map(|f| f.program.as_ref())
        .filter_map(|p| p.rsplit('\\').next())
        .map(|n| n.to_ascii_lowercase())
        .collect();
    // SAFETY: instantané des processus, structure de taille renseignée.
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if !snap.is_null() && snap as isize != -1 {
            let mut e: PROCESSENTRY32W = std::mem::zeroed();
            e.dwSize = size_of::<PROCESSENTRY32W>() as u32;
            let mut ok = Process32FirstW(snap, &mut e) != 0;
            while ok {
                let n = e.szExeFile.iter().position(|c| *c == 0).unwrap_or(e.szExeFile.len());
                let name = String::from_utf16_lossy(&e.szExeFile[..n]);
                if programs.contains(&name.to_ascii_lowercase()) {
                    out.push((e.th32ProcessID, name));
                }
                ok = Process32NextW(snap, &mut e) != 0;
            }
            windows_sys::Win32::Foundation::CloseHandle(snap);
        }
    }
    for f in &catalog.firewall {
        if let Some(s) = &f.service {
            if let Some(pid) = crate::sysconfig::service_pid(s) {
                out.push((pid, format!("service {s}")));
            }
        }
    }
    out
}

/// Connexions réseau ouvertes en ce moment par les composants de télémétrie (hors
/// boucle locale). Vide : rien ne parle.
pub fn telemetry_connections(catalog: &prism_core::privacy::Catalog) -> Vec<TelemetryConnection> {
    let pids = component_pids(catalog);
    if pids.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    // SAFETY: premier appel pour la taille, second avec un tampon aligné (Vec<u64>).
    unsafe {
        let mut size = 0u32;
        GetExtendedTcpTable(null_mut(), &mut size, 0, 2, TCP_TABLE_OWNER_PID_ALL, 0);
        if size == 0 {
            return out;
        }
        let mut buf = vec![0u64; (size as usize).div_ceil(8) + 1];
        if GetExtendedTcpTable(buf.as_mut_ptr() as *mut _, &mut size, 0, 2, TCP_TABLE_OWNER_PID_ALL, 0) != 0 {
            return out;
        }
        let table = &*(buf.as_ptr() as *const MIB_TCPTABLE_OWNER_PID);
        let rows = std::slice::from_raw_parts(table.table.as_ptr(), table.dwNumEntries as usize);
        for r in rows {
            let Some((_, component)) = pids.iter().find(|(p, _)| *p == r.dwOwningPid) else {
                continue;
            };
            let ip = std::net::Ipv4Addr::from(u32::from_be(r.dwRemoteAddr));
            if ip.is_unspecified() || ip.is_loopback() {
                continue;
            }
            let port = u16::from_be((r.dwRemotePort & 0xffff) as u16);
            let state = match r.dwState {
                2 => "en attente de réponse",
                5 => "établie",
                _ => "fermeture",
            };
            out.push(TelemetryConnection {
                component: component.clone(),
                remote: format!("{ip}:{port}"),
                state: state.into(),
            });
        }
    }
    out
}
