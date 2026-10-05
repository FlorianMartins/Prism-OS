//! Icône de Prism dans la zone de notification de Windows (les « icônes cachées » par
//! défaut sous Windows 11, donc aussi derrière le widget « Zone système » de la Prism
//! Bar). Clic : ouvre Prism ; clic du milieu : libère la RAM ; clic droit : menu.
//! Portée par la barre, qui tourne dans la session de l'utilisateur : le moteur, sous
//! le compte système, ne peut pas poser d'icône sur le bureau de l'utilisateur.

use std::cell::{Cell, RefCell};
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{HWND, LPARAM, POINT};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIIF_INFO, NIIF_NOSOUND, NIM_ADD,
    NIM_DELETE, NIM_MODIFY, NIM_SETVERSION, NIN_SELECT, NOTIFYICONDATAW, NOTIFYICON_VERSION_4,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, ChangeWindowMessageFilterEx, CreatePopupMenu, DestroyMenu, GetCursorPos, GetSystemMetrics, LoadImageW,
    PostMessageW, RegisterWindowMessageW, SetForegroundWindow, SetMenuDefaultItem, TrackPopupMenu, HICON, IMAGE_ICON,
    LR_DEFAULTCOLOR, MF_GRAYED, MF_SEPARATOR, MF_STRING, MSGFLT_ALLOW, SM_CXSMICON, SM_CYSMICON, TPM_RETURNCMD,
    TPM_RIGHTBUTTON, WM_APP, WM_CLOSE, WM_CONTEXTMENU, WM_LBUTTONUP, WM_MBUTTONUP,
};

use crate::bar_app::wide;

/// Message de rappel de l'icône (clics), envoyé par l'Explorateur à la barre.
pub const WM_TRAY: u32 = WM_APP + 5;
const ID: u32 = 1;
/// Entrée au clavier sur l'icône (Entrée, Espace) : `NIN_SELECT | NINF_KEY`.
const NIN_KEYSELECT: u32 = NIN_SELECT | 1;

thread_local! {
    static SHOWN: Cell<bool> = const { Cell::new(false) };
    static TIP: RefCell<String> = const { RefCell::new(String::new()) };
    static TASKBAR_CREATED: Cell<u32> = const { Cell::new(0) };
}

fn copy(dst: &mut [u16], s: &str) {
    let w: Vec<u16> = s.encode_utf16().take(dst.len() - 1).collect();
    dst[..w.len()].copy_from_slice(&w);
    dst[w.len()] = 0;
}

fn data(hwnd: HWND) -> NOTIFYICONDATAW {
    // SAFETY: structure de données simple, mise à zéro puis remplie.
    let mut d: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    d.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    d.hWnd = hwnd;
    d.uID = ID;
    d
}

fn icon() -> HICON {
    // SAFETY: icône n° 1 des ressources de l'exécutable (le logo), à la taille des
    // petites icônes de l'écran.
    unsafe {
        LoadImageW(
            GetModuleHandleW(null()),
            1 as _,
            IMAGE_ICON,
            GetSystemMetrics(SM_CXSMICON),
            GetSystemMetrics(SM_CYSMICON),
            LR_DEFAULTCOLOR,
        ) as HICON
    }
}

/// Message diffusé par l'Explorateur quand la barre des tâches renaît (Explorateur
/// relancé) : les icônes doivent être reposées.
pub fn taskbar_created() -> u32 {
    TASKBAR_CREATED.with(|t| {
        if t.get() == 0 {
            let name = wide("TaskbarCreated");
            // SAFETY: chaîne locale terminée par zéro.
            t.set(unsafe { RegisterWindowMessageW(name.as_ptr()) });
        }
        t.get()
    })
}

/// Montre ou retire l'icône selon le réglage.
pub fn set(hwnd: HWND, on: bool) {
    if on == SHOWN.with(|s| s.get()) {
        return;
    }
    if on {
        add(hwnd);
    } else {
        remove(hwnd);
    }
}

fn add(hwnd: HWND) {
    // La barre tourne avec les droits les plus élevés du compte : sans ces exceptions,
    // Windows bloque les messages venus de l'Explorateur (clics, renaissance de la
    // barre des tâches), qui tourne avec des droits ordinaires.
    // SAFETY: notre fenêtre ; messages enregistrés ou à nous.
    unsafe {
        for m in [WM_TRAY, taskbar_created()] {
            ChangeWindowMessageFilterEx(hwnd, m, MSGFLT_ALLOW, null_mut());
        }
    }
    let mut d = data(hwnd);
    d.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP | NIF_SHOWTIP;
    d.uCallbackMessage = WM_TRAY;
    d.hIcon = icon();
    let tip = TIP.with(|t| t.borrow().clone());
    copy(&mut d.szTip, if tip.is_empty() { "Prism OS" } else { &tip });
    // SAFETY: structure complète, durée de vie locale.
    let ok = unsafe { Shell_NotifyIconW(NIM_ADD, &d) } != 0;
    if ok {
        d.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        // SAFETY: idem.
        unsafe { Shell_NotifyIconW(NIM_SETVERSION, &d) };
    }
    SHOWN.with(|s| s.set(ok));
}

pub fn remove(hwnd: HWND) {
    if SHOWN.with(|s| s.replace(false)) {
        let d = data(hwnd);
        // SAFETY: structure locale.
        unsafe { Shell_NotifyIconW(NIM_DELETE, &d) };
    }
}

/// L'Explorateur a redémarré : l'icône est reposée (si elle était montrée).
pub fn explorer_restarted(hwnd: HWND) {
    if SHOWN.with(|s| s.replace(false)) {
        add(hwnd);
    }
}

/// Texte de l'infobulle, d'après l'état de la barre.
pub fn tip_text(game: bool, ram_percent: f32) -> String {
    format!(
        "Prism OS\n{}\nMémoire utilisée : {:.0} %",
        if game { "Mode Jeu actif" } else { "Mode Quotidien actif" },
        ram_percent
    )
}

/// Met l'infobulle à jour (seulement si le texte change).
pub fn update_tip(hwnd: HWND, text: String) {
    let changed = TIP.with(|t| {
        let mut t = t.borrow_mut();
        let changed = *t != text;
        *t = text;
        changed
    });
    if !changed || !SHOWN.with(|s| s.get()) {
        return;
    }
    let mut d = data(hwnd);
    d.uFlags = NIF_TIP | NIF_SHOWTIP;
    TIP.with(|t| copy(&mut d.szTip, &t.borrow()));
    // SAFETY: structure locale.
    unsafe { Shell_NotifyIconW(NIM_MODIFY, &d) };
}

/// Notification Windows sous l'icône (appelable depuis un autre fil).
fn notify(hwnd: isize, title: &str, body: &str) {
    let mut d = data(hwnd as HWND);
    d.uFlags = NIF_INFO;
    d.dwInfoFlags = NIIF_INFO | NIIF_NOSOUND;
    copy(&mut d.szInfoTitle, title);
    copy(&mut d.szInfo, body);
    // SAFETY: structure locale.
    unsafe { Shell_NotifyIconW(NIM_MODIFY, &d) };
}

fn gb(bytes: u64) -> String {
    format!("{:.1}", bytes as f64 / (1024.0 * 1024.0 * 1024.0)).replace('.', ",")
}

/// « Mémoire utilisée : 9,9 → 7,4 Go (−2,5 Go) ».
fn used_line(before: u64, after: u64) -> String {
    // Écart calculé sur les valeurs affichées (au dixième) : 3,8 → 3,7 dit bien −0,1.
    const STEP: u64 = 1024 * 1024 * 1024 / 10;
    let round = |b: u64| (b + STEP / 2) / STEP * STEP;
    let (b, a) = (round(before), round(after));
    let mut s = format!("Mémoire utilisée : {} → {} Go", gb(b), gb(a));
    if b > a {
        s.push_str(&format!(" (−{} Go)", gb(b - a)));
    }
    s
}

/// Libère la RAM (même nettoyage que `prism ram clean`) dans un fil à part, puis
/// annonce le résultat en notification.
fn clean_ram(hwnd: HWND) {
    let h = hwnd as isize;
    std::thread::spawn(move || {
        let cfg = prism_core::paths::load_config().unwrap_or_else(|_| prism_core::config::Config::builtin());
        let mut w = crate::WindowsPlatform::new();
        let body = match prism_core::engine::clean(&mut w, &cfg, false) {
            Ok(r) => match (r.mem_before, r.mem_after) {
                (Some(a), Some(b)) => used_line(a.used(), b.used()),
                _ => "Nettoyage fait.".to_string(),
            },
            Err(e) => format!("Échec : {e}"),
        };
        notify(h, "Prism : mémoire libérée", &body);
    });
}

/// Message de rappel de l'icône. `lp` : événement dans le mot bas (version 4).
pub fn on_message(hwnd: HWND, lp: LPARAM, game: bool) {
    match (lp as u32) & 0xFFFF {
        WM_LBUTTONUP | NIN_SELECT | NIN_KEYSELECT => crate::bar_app::open_prism_ui(),
        WM_MBUTTONUP => clean_ram(hwnd),
        WM_CONTEXTMENU => menu(hwnd, game),
        _ => {}
    }
}

fn menu(hwnd: HWND, game: bool) {
    // SAFETY: menu créé, affiché puis détruit ici.
    unsafe {
        let m = CreatePopupMenu();
        if m.is_null() {
            return;
        }
        let state = wide(if game { "Mode Jeu actif" } else { "Mode Quotidien actif" });
        AppendMenuW(m, MF_STRING | MF_GRAYED, 0, state.as_ptr());
        AppendMenuW(m, MF_SEPARATOR, 0, null());
        let items = [
            (1usize, "Ouvrir Prism"),
            (2, "Libérer la RAM maintenant"),
            (0, ""),
            (3, "Quitter la Prism Bar"),
        ];
        for (id, label) in items {
            if id == 0 {
                AppendMenuW(m, MF_SEPARATOR, 0, null());
            } else {
                let w = wide(label);
                AppendMenuW(m, MF_STRING, id, w.as_ptr());
            }
        }
        SetMenuDefaultItem(m, 1, 0);
        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        // Sans premier plan, le menu ne se ferme pas quand on clique ailleurs.
        SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(m, TPM_RETURNCMD | TPM_RIGHTBUTTON, pt.x, pt.y, 0, hwnd, null());
        DestroyMenu(m);
        match cmd {
            1 => crate::bar_app::open_prism_ui(),
            2 => clean_ram(hwnd),
            3 => {
                PostMessageW(hwnd, WM_CLOSE, 0, 0);
            }
            _ => {}
        }
    }
}
