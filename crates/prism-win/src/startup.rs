//! Applications au démarrage : clés `Run`, dossiers Démarrage, et leurs valeurs
//! `StartupApproved` (exactement ce que lit et écrit le Gestionnaire des tâches).

use std::path::PathBuf;
use std::ptr::{null, null_mut};

use prism_core::demarrage::{Entry, Source, StartupConfig};
use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_NO_MORE_ITEMS, FILETIME};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegEnumValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_BINARY, REG_EXPAND_SZ,
    REG_OPTION_NON_VOLATILE, REG_SZ, REG_VALUE_TYPE,
};
use windows_sys::Win32::System::SystemInformation::GetSystemTimeAsFileTime;

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN32: &str = r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run";
const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

struct Key(HKEY);

impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: clé ouverte possédée par cette structure.
        unsafe { RegCloseKey(self.0) };
    }
}

fn open(root: HKEY, sub: &str, access: u32) -> Option<Key> {
    let k = wide(sub);
    let mut h: HKEY = null_mut();
    // SAFETY: chaîne large terminée par zéro, sortie locale.
    (unsafe { RegOpenKeyExW(root, k.as_ptr(), 0, access, &mut h) } == 0).then_some(Key(h))
}

/// Valeurs texte d'une clé `Run` : (nom, commande).
fn run_values(root: HKEY, sub: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Some(key) = open(root, sub, KEY_QUERY_VALUE) else {
        return out;
    };
    for i in 0.. {
        let mut name = [0u16; 512];
        let mut name_len = name.len() as u32;
        let mut data = [0u16; 2048];
        let mut data_len = (data.len() * 2) as u32;
        let mut kind: REG_VALUE_TYPE = 0;
        // SAFETY: tampons locaux de capacité annoncée.
        let st = unsafe {
            RegEnumValueW(
                key.0,
                i,
                name.as_mut_ptr(),
                &mut name_len,
                null(),
                &mut kind,
                data.as_mut_ptr() as *mut u8,
                &mut data_len,
            )
        };
        if st == ERROR_NO_MORE_ITEMS {
            break;
        }
        if st != 0 || !(kind == REG_SZ || kind == REG_EXPAND_SZ) {
            continue;
        }
        let n = String::from_utf16_lossy(&name[..name_len as usize]);
        let d = String::from_utf16_lossy(&data[..(data_len as usize / 2).min(data.len())]);
        out.push((n, d.trim_end_matches('\0').to_string()));
    }
    out
}

fn folder(env: &str, rest: &str) -> Option<PathBuf> {
    std::env::var_os(env).map(|b| PathBuf::from(b).join(rest))
}

fn folder_items(dir: Option<PathBuf>) -> Vec<(String, String)> {
    let Some(dir) = dir else { return Vec::new() };
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    rd.flatten()
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .map(|e| {
            (
                e.file_name().to_string_lossy().into_owned(),
                e.path().display().to_string(),
            )
        })
        .filter(|(n, _)| !n.eq_ignore_ascii_case("desktop.ini"))
        .collect()
}

fn approved_location(source: Source) -> (HKEY, String) {
    match source {
        Source::UserRun => (HKEY_CURRENT_USER, format!(r"{APPROVED}\Run")),
        Source::MachineRun => (HKEY_LOCAL_MACHINE, format!(r"{APPROVED}\Run")),
        Source::MachineRun32 => (HKEY_LOCAL_MACHINE, format!(r"{APPROVED}\Run32")),
        Source::UserFolder => (HKEY_CURRENT_USER, format!(r"{APPROVED}\StartupFolder")),
        Source::CommonFolder => (HKEY_LOCAL_MACHINE, format!(r"{APPROVED}\StartupFolder")),
    }
}

fn read_approval(source: Source, name: &str) -> Option<Vec<u8>> {
    let (root, sub) = approved_location(source);
    let key = open(root, &sub, KEY_QUERY_VALUE)?;
    let v = wide(name);
    let mut buf = [0u8; 64];
    let mut len = buf.len() as u32;
    let mut kind: REG_VALUE_TYPE = 0;
    // SAFETY: tampon local de capacité annoncée.
    let st = unsafe { RegQueryValueExW(key.0, v.as_ptr(), null(), &mut kind, buf.as_mut_ptr(), &mut len) };
    (st == 0 && kind == REG_BINARY).then(|| buf[..len as usize].to_vec())
}

pub struct WindowsStartup;

impl StartupConfig for WindowsStartup {
    fn entries(&mut self) -> Result<Vec<Entry>, String> {
        let mut raw: Vec<(Source, String, String)> = Vec::new();
        raw.extend(
            run_values(HKEY_CURRENT_USER, RUN)
                .into_iter()
                .map(|(n, c)| (Source::UserRun, n, c)),
        );
        raw.extend(
            run_values(HKEY_LOCAL_MACHINE, RUN)
                .into_iter()
                .map(|(n, c)| (Source::MachineRun, n, c)),
        );
        raw.extend(
            run_values(HKEY_LOCAL_MACHINE, RUN32)
                .into_iter()
                .map(|(n, c)| (Source::MachineRun32, n, c)),
        );
        let user = folder("APPDATA", r"Microsoft\Windows\Start Menu\Programs\Startup");
        let common = folder("ProgramData", r"Microsoft\Windows\Start Menu\Programs\StartUp");
        raw.extend(folder_items(user).into_iter().map(|(n, c)| (Source::UserFolder, n, c)));
        raw.extend(
            folder_items(common)
                .into_iter()
                .map(|(n, c)| (Source::CommonFolder, n, c)),
        );
        Ok(raw
            .into_iter()
            .map(|(source, name, command)| {
                let approval = read_approval(source, &name);
                Entry {
                    source,
                    name,
                    command,
                    approval,
                }
            })
            .collect())
    }

    fn set_approval(&mut self, source: Source, name: &str, value: Option<&[u8]>) -> Result<(), String> {
        let (root, sub) = approved_location(source);
        let k = wide(&sub);
        let v = wide(name);
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
                return Err(format!(
                    "StartupApproved : erreur Windows {st} (droits administrateur pour une entrée machine)"
                ));
            }
            let h = Key(h);
            let st = match value {
                Some(bytes) => RegSetValueExW(h.0, v.as_ptr(), 0, REG_BINARY, bytes.as_ptr(), bytes.len() as u32),
                None => match RegDeleteValueW(h.0, v.as_ptr()) {
                    ERROR_FILE_NOT_FOUND => 0,
                    other => other,
                },
            };
            if st == 0 {
                Ok(())
            } else {
                Err(format!("StartupApproved\\{name} : erreur Windows {st}"))
            }
        }
    }

    fn now_filetime(&mut self) -> u64 {
        // SAFETY: sortie locale.
        unsafe {
            let mut ft: FILETIME = std::mem::zeroed();
            GetSystemTimeAsFileTime(&mut ft);
            ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64
        }
    }
}
