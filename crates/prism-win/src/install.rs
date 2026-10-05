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

/// Nombre d'autres processus `exe` (soi-même exclu).
pub fn other_instances(exe: &str) -> usize {
    let me = std::process::id();
    let mut n = 0;
    // SAFETY: instantané des processus, refermé ici.
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
            if String::from_utf16_lossy(&e.szExeFile[..len]).eq_ignore_ascii_case(exe) && e.th32ProcessID != me {
                n += 1;
            }
            ok = Process32NextW(snap, &mut e) != 0;
        }
        CloseHandle(snap);
    }
    n
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

/// Boîte de message Windows (erreur visible même sans console).
pub fn error_box(title: &str, text: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
    let (t, m) = (wide(title), wide(text));
    // SAFETY: chaînes larges terminées par zéro.
    unsafe { MessageBoxW(std::ptr::null_mut(), m.as_ptr(), t.as_ptr(), MB_OK | MB_ICONERROR) };
}

fn schtasks_hidden(args: &[&str]) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let out = std::process::Command::new("schtasks.exe")
        .args(args)
        .creation_flags(0x0800_0000)
        .output()
        .map_err(|e| format!("schtasks.exe : {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "Planificateur de tâches : {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// Crée (ou remplace) une tâche planifiée à partir de sa définition XML (UTF-16, comme
/// l'exige le Planificateur).
fn create_task_xml(name: &str, xml: &str) -> Result<(), String> {
    let path = std::env::temp_dir().join(format!("prism-tache-{}.xml", name.replace(' ', "-")));
    let mut bytes = vec![0xFF, 0xFE];
    for u in xml.encode_utf16() {
        bytes.extend_from_slice(&u.to_le_bytes());
    }
    std::fs::write(&path, bytes).map_err(|e| format!("{} : {e}", path.display()))?;
    let r = schtasks_hidden(&["/Create", "/TN", name, "/XML", &path.display().to_string(), "/F"]);
    let _ = std::fs::remove_file(&path);
    r
}

/// Démarrage automatique : moteur au démarrage de Windows (compte système, avant la
/// connexion), barre à chaque ouverture de session. Droits administrateur requis.
pub fn autostart_install(dir: &str) -> Result<(), String> {
    use prism_core::autostart::{bar_task_xml, engine_task_xml, BAR_TASK, ENGINE_TASK};
    let dir = dir.trim_end_matches('\\');
    create_task_xml(ENGINE_TASK, &engine_task_xml(dir))?;
    create_task_xml(BAR_TASK, &bar_task_xml(dir))
}

pub fn autostart_remove() {
    use prism_core::autostart::{BAR_TASK, ENGINE_TASK};
    let _ = schtasks_hidden(&["/Delete", "/TN", ENGINE_TASK, "/F"]);
    let _ = schtasks_hidden(&["/Delete", "/TN", BAR_TASK, "/F"]);
}

pub fn autostart_enabled() -> bool {
    schtasks_hidden(&["/Query", "/TN", prism_core::autostart::ENGINE_TASK]).is_ok()
}

/// Lance tout de suite une tâche (le moteur sous le compte système, par exemple).
pub fn run_task(name: &str) -> Result<(), String> {
    schtasks_hidden(&["/Run", "/TN", name])
}

/// Session de la console où quelqu'un est connecté (`None` : écran de connexion, ou
/// personne). Sert au moteur lancé sous le compte système avant la connexion.
pub fn console_user_session() -> Option<u32> {
    use windows_sys::Win32::System::RemoteDesktop::{
        WTSFreeMemory, WTSGetActiveConsoleSessionId, WTSQuerySessionInformationW, WTSUserName,
        WTS_CURRENT_SERVER_HANDLE,
    };
    // SAFETY: tampon alloué par Windows, libéré par WTSFreeMemory.
    unsafe {
        let id = WTSGetActiveConsoleSessionId();
        if id == u32::MAX {
            return None;
        }
        let mut buf: windows_sys::core::PWSTR = null_mut();
        let mut len = 0u32;
        if WTSQuerySessionInformationW(WTS_CURRENT_SERVER_HANDLE, id, WTSUserName, &mut buf, &mut len) == 0
            || buf.is_null()
        {
            return None;
        }
        let has_user = *buf != 0;
        WTSFreeMemory(buf as *mut _);
        has_user.then_some(id)
    }
}

/// Dossier des réglages (`%LOCALAPPDATA%\Prism`) de la personne connectée à la console,
/// lu depuis le compte système (jeton de session + dossier de profil).
pub fn console_user_dir() -> Option<std::path::PathBuf> {
    use windows_sys::Win32::System::RemoteDesktop::WTSQueryUserToken;
    use windows_sys::Win32::UI::Shell::GetUserProfileDirectoryW;
    let session = console_user_session()?;
    // SAFETY: jeton fermé après usage ; tampon local de taille annoncée.
    unsafe {
        let mut token = null_mut();
        if WTSQueryUserToken(session, &mut token) == 0 {
            return None;
        }
        let mut buf = [0u16; 260];
        let mut len = buf.len() as u32;
        let ok = GetUserProfileDirectoryW(token, buf.as_mut_ptr(), &mut len) != 0;
        CloseHandle(token);
        if !ok {
            return None;
        }
        let n = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
        let profile = String::from_utf16_lossy(&buf[..n]);
        Some(
            std::path::Path::new(&profile)
                .join("AppData")
                .join("Local")
                .join("Prism"),
        )
    }
}

/// Événement « arrête-toi » du moteur : le moteur tourne sous le compte système, sans
/// console ; l'interface (session de l'utilisateur) le signale pour l'arrêter proprement
/// (tout est rendu à Windows avant de quitter).
const ENGINE_STOP: &str = "Global\\PrismEngineStop";

/// Côté moteur : crée l'événement, signalable par les utilisateurs de la session
/// (`IU` : SYNCHRONIZE + EVENT_MODIFY_STATE), et attend dans un fil ; `on_stop` est
/// appelé une fois signalé.
pub fn engine_stop_listener(on_stop: impl FnOnce() + Send + 'static) -> bool {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
    use windows_sys::Win32::System::Threading::{CreateEventW, ResetEvent, WaitForSingleObject, INFINITE};
    let sddl: Vec<u16> = "D:(A;;GA;;;SY)(A;;GA;;;BA)(A;;0x00100002;;;IU)"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let name: Vec<u16> = ENGINE_STOP.encode_utf16().chain(Some(0)).collect();
    // SAFETY: descripteur alloué par Windows puis libéré ; l'événement vit jusqu'à la
    // fin du processus (le fil d'attente le garde).
    unsafe {
        let mut sd = null_mut();
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.as_ptr(), SDDL_REVISION_1, &mut sd, null_mut())
            == 0
        {
            return false;
        }
        let sa = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: sd,
            bInheritHandle: 0,
        };
        let ev = CreateEventW(&sa, 1, 0, name.as_ptr());
        LocalFree(sd as _);
        if ev.is_null() {
            return false;
        }
        // Signal resté d'un arrêt précédent : effacé.
        ResetEvent(ev);
        let h = ev as isize;
        std::thread::spawn(move || {
            WaitForSingleObject(h as _, INFINITE);
            on_stop();
        });
        true
    }
}

/// Côté interface : demande au moteur de s'arrêter. Faux s'il ne tourne pas.
pub fn engine_stop() -> bool {
    use windows_sys::Win32::System::Threading::{OpenEventW, SetEvent, EVENT_MODIFY_STATE};
    let name: Vec<u16> = ENGINE_STOP.encode_utf16().chain(Some(0)).collect();
    // SAFETY: poignée ouverte puis fermée ici.
    unsafe {
        let ev = OpenEventW(EVENT_MODIFY_STATE, 0, name.as_ptr());
        if ev.is_null() {
            return false;
        }
        let ok = SetEvent(ev) != 0;
        CloseHandle(ev);
        ok
    }
}
