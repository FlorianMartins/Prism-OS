//! Installation et désinstallation : PATH du système, session de l'utilisateur, arrêt
//! des autres instances de Prism.

use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{CloseHandle, ERROR_FILE_NOT_FOUND, LPARAM, WPARAM};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY, HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE,
    KEY_SET_VALUE, REG_EXPAND_SZ, REG_SZ, REG_VALUE_TYPE,
};
use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows_sys::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
};

const ENV_KEY: &str = "SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Le processus tourne-t-il dans la session des services (installateur MSI, SYSTEM) ?
pub fn in_service_session() -> bool {
    let mut session = 0u32;
    // SAFETY: sortie locale.
    let ok = unsafe { ProcessIdToSessionId(std::process::id(), &mut session) } != 0;
    ok && session == 0
}

/// PATH du système, lu tel quel (`%SystemRoot%` non développé).
fn read_path() -> Result<String, String> {
    let k = wide(ENV_KEY);
    let v = wide("Path");
    // SAFETY: clé ouverte puis fermée ici ; tampon de taille annoncée.
    unsafe {
        let mut h: HKEY = null_mut();
        let st = RegOpenKeyExW(HKEY_LOCAL_MACHINE, k.as_ptr(), 0, KEY_QUERY_VALUE, &mut h);
        if st != 0 {
            return Err(format!("environnement du système : erreur {st}"));
        }
        let mut kind: REG_VALUE_TYPE = 0;
        let mut buf = vec![0u16; 32_768];
        let mut len = (buf.len() * 2) as u32;
        let st = RegQueryValueExW(h, v.as_ptr(), null(), &mut kind, buf.as_mut_ptr() as *mut u8, &mut len);
        RegCloseKey(h);
        match st {
            0 if kind == REG_EXPAND_SZ || kind == REG_SZ => {
                let n = (len as usize / 2).min(buf.len());
                Ok(String::from_utf16_lossy(&buf[..n]).trim_end_matches('\0').to_string())
            }
            ERROR_FILE_NOT_FOUND => Ok(String::new()),
            0 => Err(format!("PATH : type de valeur inattendu ({kind})")),
            code => Err(format!("PATH : erreur {code}")),
        }
    }
}

/// Écrit le PATH en REG_EXPAND_SZ (son type d'origine : en texte simple, les entrées
/// `%SystemRoot%` ne seraient plus développées), puis prévient les programmes.
fn write_path(path: &str) -> Result<(), String> {
    let k = wide(ENV_KEY);
    let v = wide("Path");
    let data = wide(path);
    // SAFETY: clé ouverte puis fermée ici ; données de taille exacte.
    unsafe {
        let mut h: HKEY = null_mut();
        let st = RegOpenKeyExW(HKEY_LOCAL_MACHINE, k.as_ptr(), 0, KEY_SET_VALUE, &mut h);
        if st != 0 {
            return Err(format!(
                "environnement du système : erreur {st} (droits administrateur ?)"
            ));
        }
        let st = RegSetValueExW(
            h,
            v.as_ptr(),
            0,
            REG_EXPAND_SZ,
            data.as_ptr() as *const u8,
            (data.len() * 2) as u32,
        );
        RegCloseKey(h);
        if st != 0 {
            return Err(format!("PATH : écriture refusée (erreur {st})"));
        }
        let env = wide("Environment");
        let mut result = 0usize;
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0 as WPARAM,
            env.as_ptr() as LPARAM,
            SMTO_ABORTIFHUNG,
            2000,
            &mut result,
        );
    }
    Ok(())
}

/// Ajoute `dir` au PATH du système ; `false` s'il y était déjà.
pub fn path_add(dir: &str) -> Result<bool, String> {
    match prism_core::paths::path_with(&read_path()?, dir) {
        Some(p) => write_path(&p).map(|()| true),
        None => Ok(false),
    }
}

/// Retire `dir` du PATH du système ; `false` s'il n'y était pas.
pub fn path_remove(dir: &str) -> Result<bool, String> {
    match prism_core::paths::path_without(&read_path()?, dir) {
        Some(p) => write_path(&p).map(|()| true),
        None => Ok(false),
    }
}

/// Arrête les autres processus `exe` (ex. `prism.exe` lancé en `watch`), jamais
/// soi-même. Renvoie leur nombre.
pub fn stop_other_instances(exe: &str) -> usize {
    let me = std::process::id();
    let mut n = 0;
    // SAFETY: instantané des processus ; chaque handle ouvert est refermé.
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap.is_null() || snap as isize == -1 {
            return 0;
        }
        let mut e: PROCESSENTRY32W = std::mem::zeroed();
        e.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = Process32FirstW(snap, &mut e) != 0;
        while ok {
            let len = e.szExeFile.iter().position(|c| *c == 0).unwrap_or(e.szExeFile.len());
            let name = String::from_utf16_lossy(&e.szExeFile[..len]);
            if name.eq_ignore_ascii_case(exe) && e.th32ProcessID != me {
                let h = OpenProcess(PROCESS_TERMINATE, 0, e.th32ProcessID);
                if !h.is_null() {
                    if TerminateProcess(h, 0) != 0 {
                        n += 1;
                    }
                    CloseHandle(h);
                }
            }
            ok = Process32NextW(snap, &mut e) != 0;
        }
        CloseHandle(snap);
    }
    n
}
