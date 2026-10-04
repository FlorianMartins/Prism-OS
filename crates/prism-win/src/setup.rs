//! Outils du programme d'installation (`prism-setup`) : installation du paquet MSI par
//! l'API de Windows Installer avec suivi de l'avancement, version déjà installée,
//! sélecteur de dossier, ouverture de Prism sans droits administrateur.

use std::ffi::c_void;
use std::ptr::{null, null_mut};

use windows_sys::core::PCWSTR;
use windows_sys::Win32::System::ApplicationInstallationAndServicing::{
    MsiEnableLogW, MsiEnumRelatedProductsW, MsiGetProductInfoW, MsiInstallProductW, MsiSetExternalUIW,
    MsiSetInternalUI, INSTALLLOGMODE_ACTIONSTART, INSTALLLOGMODE_PROGRESS, INSTALLLOGMODE_VERBOSE, INSTALLUILEVEL_NONE,
};
use windows_sys::Win32::UI::Shell::{
    SHBrowseForFolderW, SHGetPathFromIDListW, BIF_NEWDIALOGSTYLE, BIF_RETURNONLYFSDIRS, BROWSEINFOW,
};

/// Code de mise à niveau du paquet (installer/prism.wxs).
const UPGRADE_CODE: &str = "{9655D7B5-C281-4A1E-8947-4BAF353C50F5}";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Version de Prism déjà installée (par le même paquet), s'il y en a une.
pub fn installed_version() -> Option<String> {
    let code = wide(UPGRADE_CODE);
    let mut product = [0u16; 39];
    // SAFETY: tampons de taille documentée (GUID : 38 caractères + zéro).
    unsafe {
        if MsiEnumRelatedProductsW(code.as_ptr(), 0, 0, product.as_mut_ptr()) != 0 {
            return None;
        }
        let attr = wide("VersionString");
        let mut buf = [0u16; 64];
        let mut len = buf.len() as u32;
        if MsiGetProductInfoW(product.as_ptr(), attr.as_ptr(), buf.as_mut_ptr(), &mut len) != 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}

/// Dossier de l'installation existante (noté par le paquet), sinon `Program Files\Prism`.
pub fn default_folder() -> String {
    if let Ok(Some(prism_core::allege::RegData::Text(t))) =
        crate::sysconfig::reg_get("HKLM\\Software\\Prism OS", "InstallFolder")
    {
        if !t.is_empty() {
            return t.trim_end_matches('\\').to_string();
        }
    }
    let pf = std::env::var("ProgramFiles").unwrap_or_else(|_| "C:\\Program Files".into());
    format!("{pf}\\Prism")
}

/// Sélecteur de dossier de Windows. `None` : annulé.
pub fn pick_folder(title: &str) -> Option<String> {
    let t = wide(title);
    // SAFETY: structure locale ; la liste d'identifiants rendue est libérée.
    unsafe {
        let mut bi: BROWSEINFOW = std::mem::zeroed();
        bi.lpszTitle = t.as_ptr();
        bi.ulFlags = BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE;
        let list = SHBrowseForFolderW(&bi);
        if list.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let ok = SHGetPathFromIDListW(list, buf.as_mut_ptr()) != 0;
        windows_sys::Win32::System::Com::CoTaskMemFree(list as *const c_void);
        if !ok {
            return None;
        }
        let n = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..n]))
    }
}

/// Avancement remonté pendant l'installation.
#[derive(Clone, Copy, Debug)]
pub enum Event {
    /// 0 à 1.
    Fraction(f32),
    /// Nom d'action de Windows Installer (InstallFiles, CreateShortcuts…).
    Action(&'static str),
}

/// Actions de Windows Installer qui marquent une étape visible.
const ACTIONS: [&str; 8] = [
    "InstallValidate",
    "RemoveExistingProducts",
    "InstallFiles",
    "PrismAfterInstall",
    "CreateShortcuts",
    "WriteRegistryValues",
    "RegisterProduct",
    "InstallFinalize",
];

struct Ctx<'a> {
    on: &'a mut dyn FnMut(Event),
    total: f64,
    done: f64,
    forward: bool,
    /// Phases de progression : la première (préparation du script) compte pour 20 %.
    phase: u32,
    shown: f32,
}

unsafe extern "system" fn handler(ctx: *mut c_void, kind: u32, msg: PCWSTR) -> i32 {
    if ctx.is_null() || msg.is_null() {
        return 0;
    }
    let c = &mut *(ctx as *mut Ctx);
    let len = (0..).take_while(|i| *msg.add(*i) != 0).count();
    let text = String::from_utf16_lossy(std::slice::from_raw_parts(msg, len));
    match kind & 0xFF00_0000 {
        // INSTALLMESSAGE_PROGRESS : « 1: type 2: valeur 3: sens … »
        0x0A00_0000 => {
            let mut f = [0f64; 4];
            for part in text.split_whitespace().collect::<Vec<_>>().chunks(2) {
                if let [k, v] = part {
                    if let (Some(i), Ok(v)) = (k.trim_end_matches(':').parse::<usize>().ok(), v.parse::<f64>()) {
                        if (1..=4).contains(&i) {
                            f[i - 1] = v;
                        }
                    }
                }
            }
            match f[0] as u32 {
                0 => {
                    c.total = f[1].max(1.0);
                    c.done = 0.0;
                    c.forward = f[2] == 0.0;
                    c.phase += 1;
                }
                2 => {
                    c.done += if c.forward { f[1] } else { -f[1] };
                }
                _ => {}
            }
            let local = (c.done / c.total).clamp(0.0, 1.0) as f32;
            let overall = if c.phase <= 1 { local * 0.2 } else { 0.2 + 0.8 * local };
            // Jamais en arrière à l'écran.
            if overall > c.shown {
                c.shown = overall;
                (c.on)(Event::Fraction(overall));
            }
        }
        // INSTALLMESSAGE_ACTIONSTART : « Action 17:42:00: InstallFiles. … »
        0x0800_0000 => {
            if let Some(a) = ACTIONS.iter().find(|a| text.contains(&format!(" {a}."))) {
                (c.on)(Event::Action(a));
            }
        }
        _ => {}
    }
    0
}

/// Installe le paquet `msi` dans `folder`, sans fenêtre de Windows Installer (c'est
/// l'interface de Prism qui montre l'avancement). Journal : `log`.
pub fn install(
    msi: &std::path::Path,
    folder: &str,
    log: &std::path::Path,
    on: &mut dyn FnMut(Event),
) -> Result<(), String> {
    let path = wide(&msi.display().to_string());
    let cmd = wide(&format!(
        "INSTALLFOLDER=\"{}\\\" PRISM_SETUP=1 REBOOT=ReallySuppress",
        folder.trim_end_matches('\\')
    ));
    let logw = wide(&log.display().to_string());
    let mut ctx = Ctx {
        on,
        total: 1.0,
        done: 0.0,
        forward: true,
        phase: 0,
        shown: 0.0,
    };
    // SAFETY: le contexte vit jusqu'à la fin de MsiInstallProductW (appel synchrone),
    // le gestionnaire est retiré ensuite.
    let code = unsafe {
        MsiSetInternalUI(INSTALLUILEVEL_NONE, null_mut());
        MsiEnableLogW(INSTALLLOGMODE_VERBOSE as u32, logw.as_ptr(), 0);
        MsiSetExternalUIW(
            Some(handler),
            (INSTALLLOGMODE_PROGRESS | INSTALLLOGMODE_ACTIONSTART) as u32,
            &mut ctx as *mut Ctx as *const c_void,
        );
        let r = MsiInstallProductW(path.as_ptr(), cmd.as_ptr());
        MsiSetExternalUIW(None, 0, null());
        r
    };
    match code {
        0 | 3010 => Ok(()),
        1602 => Err("installation annulée".into()),
        1618 => Err("une autre installation Windows est en cours ; réessayez dans un instant".into()),
        c => Err(format!(
            "Windows Installer a refusé (code {c}). Journal : {}",
            log.display()
        )),
    }
}

/// Ouvre Prism sans droits administrateur (par l'Explorateur, au nom de l'utilisateur),
/// même depuis l'installateur qui, lui, en a.
pub fn launch_unelevated(exe: &std::path::Path) {
    let _ = std::process::Command::new("explorer.exe").arg(exe).spawn();
}
