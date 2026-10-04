//! Prism Bar : barre native Win32 (GDI), ancrée par `SHAppBarMessage` sur le bord
//! choisi. Légère par construction : elle tourne en permanence.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};

use prism_core::bar::{bar_rect, layout, BarConfig, DeskKind, Edge, History, Placed, Rect, Widget};
use prism_core::etat::Etat;
use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DwmSetWindowAttribute, DWMWA_CLOAKED};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::SystemInformation::GetLocalTime;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::HiDpi::{
    GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_LWIN,
};
use windows_sys::Win32::UI::Shell::{
    SHAppBarMessage, ABE_BOTTOM, ABE_LEFT, ABE_RIGHT, ABE_TOP, ABM_GETSTATE, ABM_NEW, ABM_QUERYPOS, ABM_REMOVE,
    ABM_SETPOS, ABM_SETSTATE, ABN_FULLSCREENAPP, ABN_POSCHANGED, ABS_AUTOHIDE, APPBARDATA,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::metrics::{human_rate, Metrics, Sample};

const CLASS: &str = "PrismBar";
const DESK_CLASS: &str = "PrismWidget";
const WM_APPBAR: u32 = WM_APP + 1;
const TIMER_ID: usize = 1;

// Couleurs (BGR pour GDI) — mêmes teintes que l'interface.
const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    r as u32 | ((g as u32) << 8) | ((b as u32) << 16)
}
/// Couleurs de la barre, tirées du thème (`bar.json`) ; relues à chaque changement.
#[derive(Clone, Copy)]
struct Colors {
    bg: COLORREF,
    item: COLORREF,
    item_active: COLORREF,
    text: COLORREF,
    muted: COLORREF,
    accent: COLORREF,
    ok: COLORREF,
    warn: COLORREF,
}

impl Colors {
    fn from_theme(t: &prism_core::theme::ThemeConfig) -> Colors {
        let p = t.palette();
        let c = |v: [u8; 3]| rgb(v[0], v[1], v[2]);
        Colors {
            bg: c(p.bg),
            item: c(p.card_hi),
            item_active: c(p.accent_dim),
            text: c(p.text),
            muted: c(p.muted),
            accent: c(p.accent),
            ok: c(p.ok),
            warn: c(p.warn),
        }
    }
}

thread_local! {
    static COLORS: std::cell::Cell<Option<Colors>> = const { std::cell::Cell::new(None) };
}

fn colors() -> Colors {
    COLORS
        .with(|c| c.get())
        .unwrap_or_else(|| Colors::from_theme(&prism_core::theme::ThemeConfig::default()))
}

fn set_colors(t: &prism_core::theme::ThemeConfig) {
    COLORS.with(|c| c.set(Some(Colors::from_theme(t))));
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

struct DeskWin {
    hwnd: HWND,
    kind: DeskKind,
}

struct TaskWin {
    hwnd: HWND,
    title: String,
    pid: u32,
}

struct Bar {
    hwnd: HWND,
    cfg: BarConfig,
    cfg_stamp: Option<std::time::SystemTime>,
    metrics: Metrics,
    sample: Sample,
    cpu: History,
    ram: History,
    gpu: History,
    windows: Vec<TaskWin>,
    /// Une barre par écran ; la première (fenêtre principale `hwnd`) est sur l'écran
    /// principal.
    panels: Vec<Panel>,
    game: bool,
    /// Widgets du bureau masqués (plein écran).
    desk_hidden: bool,
    scale: f32,
    desk: Vec<DeskWin>,
    /// Règles du Mode Jeu, pour ne jamais rendre un jeu transparent.
    game_cfg: prism_core::config::Config,
    /// Fenêtres rendues transparentes : style étendu d'origine et opacité posée.
    translucent: std::collections::HashMap<isize, (isize, u8)>,
    overlay: Option<crate::fx_overlay::Overlay>,
    /// Dernière copie de chaque fenêtre (pour réduire, restaurer, fermer avec effet).
    snaps: HashMap<isize, crate::fx_overlay::Snap>,
    /// Événements provoqués par Prism lui-même (fenêtre, événement attendu) : ignorés
    /// une fois quand Windows les renvoie, quel que soit le délai de traitement.
    skip: HashMap<(isize, u32), Instant>,
    stats: Vec<prism_core::fx::FxStat>,
    /// Fenêtre en cours de déplacement, en gélatine.
    drag: Option<DragFx>,
    /// Dernier cadre connu de chaque fenêtre (départ de la glisse quand sa taille change).
    geo: HashMap<isize, Rect>,
    /// Abonnement aux changements de position, limité à l'appli au premier plan
    /// (pas aux mouvements de souris de tout le système) : (abonnement, processus).
    loc_hook: Option<(isize, u32)>,
    /// Fenêtres en tuiles.
    tiler: crate::tiler::Tiler,
}

/// Gélatine pendant un déplacement : la vraie fenêtre, rendue invisible, est
/// déplacée par Windows ; Prism dessine sa copie déformée par-dessus.
struct DragFx {
    hwnd: HWND,
    /// Style étendu d'origine (remis à la fin).
    ex: isize,
    snap: crate::fx_overlay::Snap,
    wob: prism_core::wobbly::Wobbly,
    frame: prism_core::fx::Image,
    margin: i32,
    last: Instant,
    started: Instant,
    released: Option<Instant>,
    /// Thread de la fenêtre : on lui demande s'il est encore dans sa boucle de déplacement.
    thread: u32,
    meter: crate::fx_overlay::Meter,
    /// Position de la fenêtre à l'image précédente.
    pos: (i32, i32),
}

/// Le thread est-il dans une boucle de déplacement/redimensionnement ?
fn in_move_size(thread: u32) -> bool {
    // SAFETY: structure de sortie locale, taille renseignée.
    unsafe {
        let mut info: GUITHREADINFO = std::mem::zeroed();
        info.cbSize = size_of::<GUITHREADINFO>() as u32;
        GetGUIThreadInfo(thread, &mut info) != 0 && info.flags & GUI_INMOVESIZE != 0
    }
}

/// La barre d'un écran.
struct Panel {
    hwnd: HWND,
    /// Écran (poignée) et son rectangle complet.
    monitor: isize,
    rect: Rect,
    scale: f32,
    items: Vec<Placed>,
    /// Indices dans `Bar::windows` des fenêtres montrées par cette barre.
    wins: Vec<usize>,
    /// Masquée pendant un plein écran sur son écran.
    hidden: bool,
}

/// Écrans branchés : (poignée, rectangle), l'écran principal en premier.
fn monitors() -> Vec<(isize, Rect)> {
    unsafe extern "system" fn cb(mon: HMONITOR, _dc: HDC, _r: *mut RECT, lp: LPARAM) -> windows_sys::core::BOOL {
        let out = &mut *(lp as *mut Vec<(isize, Rect, bool)>);
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(mon, &mut mi) != 0 {
            let r = mi.rcMonitor;
            out.push((
                mon as isize,
                Rect {
                    left: r.left,
                    top: r.top,
                    right: r.right,
                    bottom: r.bottom,
                },
                mi.dwFlags & MONITORINFOF_PRIMARY != 0,
            ));
        }
        1
    }
    let mut out: Vec<(isize, Rect, bool)> = Vec::new();
    // SAFETY: le pointeur vers `out` reste valide pendant l'énumération synchrone.
    unsafe { EnumDisplayMonitors(null_mut(), null(), Some(cb), &mut out as *mut _ as LPARAM) };
    out.sort_by_key(|(_, r, primary)| (!*primary, r.left, r.top));
    out.into_iter().map(|(m, r, _)| (m, r)).collect()
}

fn monitor_scale(mon: isize) -> f32 {
    use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    let (mut x, mut y) = (96u32, 96u32);
    // SAFETY: sorties locales.
    unsafe { GetDpiForMonitor(mon as HMONITOR, MDT_EFFECTIVE_DPI, &mut x, &mut y) };
    x.max(96) as f32 / 96.0
}

/// Fenêtre d'une barre (une par écran), enregistrée auprès du shell.
fn create_panel_window() -> HWND {
    let class = wide(CLASS);
    let title = wide("Prism Bar");
    // SAFETY: classe enregistrée par `run`, fenêtre de ce processus.
    unsafe {
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_NOACTIVATE,
            class.as_ptr(),
            title.as_ptr(),
            WS_POPUP,
            0,
            0,
            100,
            40,
            null_mut(),
            null_mut(),
            GetModuleHandleW(null()),
            null(),
        );
        if !hwnd.is_null() {
            let mut d = appbar_data(hwnd);
            SHAppBarMessage(ABM_NEW, &mut d);
        }
        hwnd
    }
}

/// Ce qu'on fait à la fenêtre : apparition (ouverture, restauration) ou disparition.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FxKind {
    Appear,
    Disappear,
}

thread_local! {
    static BAR: RefCell<Option<Bar>> = const { RefCell::new(None) };
    /// Événements de fenêtres en attente pour les effets (traités hors de tout
    /// emprunt de `BAR` : une animation ne peut jamais être réentrée).
    static FX_QUEUE: RefCell<Vec<(u32, isize, Instant)>> = const { RefCell::new(Vec::new()) };
    static BAR_HWND: std::cell::Cell<isize> = const { std::cell::Cell::new(0) };
}

/// Accès à l'état de la barre ; `None` si elle est déjà en cours d'utilisation
/// (un rappel Windows arrivé pendant un traitement ne doit pas paniquer).
fn with_bar<R>(f: impl FnOnce(&mut Bar) -> R) -> Option<R> {
    BAR.with(|b| b.try_borrow_mut().ok()?.as_mut().map(f))
}

/// Diagnostic activé (`fx-debug.log` existe) : relu chaque seconde par la barre.
static FX_DEBUG: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Journal de diagnostic des effets, seulement si `fx-debug.log` existe déjà.
fn fx_log(msg: impl FnOnce() -> String) {
    use std::io::Write;
    if !FX_DEBUG.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let path = prism_core::paths::data_dir().join("fx-debug.log");
    if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(path) {
        let _ = writeln!(f, "{}", msg());
    }
}

/// Diagnostic : écrit une image affichée (PPM) si `fx-debug.log` existe.
fn fx_dump(img: &prism_core::fx::Image, name: &str) {
    if !FX_DEBUG.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let dir = prism_core::paths::data_dir();
    let mut ppm = format!("P6 {} {} 255\n", img.width, img.height).into_bytes();
    for px in &img.pixels {
        let a = px >> 24;
        // Fond gris sous les pixels transparents.
        let mix = |c: u32| (c + 0x50 * (255 - a) / 255).min(255) as u8;
        ppm.extend([mix((px >> 16) & 0xff), mix((px >> 8) & 0xff), mix(px & 0xff)]);
    }
    let _ = std::fs::write(dir.join(format!("{name}.ppm")), ppm);
}

/// Raccourcis des tuiles (Win+Ctrl+Alt+…). Vérifié en VM (Windows 11 26H2) : Windows
/// prend déjà Win+Alt+W, F, Y et les flèches, et la Game Bar Win+Alt+B, G, K, M, R, T ;
/// Win+Ctrl+Alt est entièrement libre, et ne se confond pas avec AltGr (qui ne
/// comprend jamais la touche Windows).
const HOTKEYS: [(i32, u32, &str); 8] = [
    (1, 0x57, "Win+Ctrl+Alt+W : tuiles oui / non"),
    (2, 0x20, "Win+Ctrl+Alt+Espace : disposition suivante"),
    (3, 0x0D, "Win+Ctrl+Alt+Entrée : fenêtre active en principale"),
    (4, 0x25, "Win+Ctrl+Alt+Gauche : fenêtre précédente"),
    (5, 0x27, "Win+Ctrl+Alt+Droite : fenêtre suivante"),
    (6, 0x26, "Win+Ctrl+Alt+Haut : agrandir la principale"),
    (7, 0x28, "Win+Ctrl+Alt+Bas : réduire la principale"),
    (8, 0x46, "Win+Ctrl+Alt+F : fenêtre active flottante / en tuile"),
];

const WM_FX: u32 = WM_APP + 3;
/// Image suivante de la gélatine de déplacement (message que la barre se poste à
/// elle-même : sa boucle de messages continue entre deux images).
const WM_FX_DRAG: u32 = WM_APP + 4;
/// Démonstration mesurée des effets (`prism fx demo`).
pub const WM_FX_DEMO: u32 = WM_APP + 2;
const EVENT_SYSTEM_FOREGROUND_ID: u32 = 0x0003;
const EVENT_SYSTEM_MOVESIZESTART_ID: u32 = 0x000A;
const EVENT_SYSTEM_MOVESIZEEND_ID: u32 = 0x000B;
const EVENT_SYSTEM_MINIMIZESTART_ID: u32 = 0x0016;
const EVENT_SYSTEM_MINIMIZEEND_ID: u32 = 0x0017;
const EVENT_OBJECT_SHOW_ID: u32 = 0x8002;
const EVENT_OBJECT_HIDE_ID: u32 = 0x8003;
const EVENT_OBJECT_LOCATIONCHANGE_ID: u32 = 0x800B;

fn edge_code(e: Edge) -> u32 {
    match e {
        Edge::Top => ABE_TOP,
        Edge::Bottom => ABE_BOTTOM,
        Edge::Left => ABE_LEFT,
        Edge::Right => ABE_RIGHT,
    }
}

fn client_size(hwnd: HWND) -> (i32, i32) {
    let mut r = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: sortie locale.
    unsafe { GetClientRect(hwnd, &mut r) };
    (r.right - r.left, r.bottom - r.top)
}

fn appbar_data(hwnd: HWND) -> APPBARDATA {
    APPBARDATA {
        cbSize: size_of::<APPBARDATA>() as u32,
        hWnd: hwnd,
        uCallbackMessage: WM_APPBAR,
        uEdge: 0,
        rc: RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        },
        lParam: 0,
    }
}

fn taskbar_hwnd() -> HWND {
    let c = wide("Shell_TrayWnd");
    // SAFETY: nom de classe local.
    unsafe { FindWindowW(c.as_ptr(), null()) }
}

/// État de la barre Windows avant que Prism la masque, gardé sur disque : une barre
/// arrêtée de force ne doit pas faire oublier l'état d'origine (sinon la suivante
/// noterait « masquée » comme état d'origine — vu en VM).
fn taskbar_state_path() -> std::path::PathBuf {
    prism_core::paths::data_dir().join("barre-windows.txt")
}

fn taskbar_state() -> Option<u32> {
    let tb = taskbar_hwnd();
    if tb.is_null() {
        return None;
    }
    let mut d = appbar_data(tb);
    // SAFETY: structure locale initialisée avec sa taille.
    Some(unsafe { SHAppBarMessage(ABM_GETSTATE, &mut d) } as u32)
}

fn set_taskbar_state(state: u32) {
    let tb = taskbar_hwnd();
    if tb.is_null() {
        return;
    }
    let mut d = appbar_data(tb);
    d.lParam = state as LPARAM;
    // SAFETY: structure locale initialisée avec sa taille.
    unsafe { SHAppBarMessage(ABM_SETSTATE, &mut d) };
}

/// Masque automatiquement la barre des tâches de Windows (son option « Masquer
/// automatiquement »), en notant son état d'origine une seule fois.
fn hide_taskbar() {
    let path = taskbar_state_path();
    if !path.exists() {
        if let Some(cur) = taskbar_state() {
            let _ = std::fs::create_dir_all(prism_core::paths::data_dir());
            let _ = std::fs::write(&path, cur.to_string());
        }
    }
    set_taskbar_state(ABS_AUTOHIDE);
}

/// Remet la barre des tâches de Windows dans son état d'origine.
fn show_taskbar() {
    let path = taskbar_state_path();
    let saved = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| t.trim().parse::<u32>().ok());
    let target = saved.unwrap_or_else(|| taskbar_state().unwrap_or(0) & !ABS_AUTOHIDE);
    set_taskbar_state(target);
    let _ = std::fs::remove_file(path);
}

impl Bar {
    fn scaled(&self, v: u32) -> i32 {
        (v as f32 * self.scale).round() as i32
    }

    /// Une barre par écran voulu : crée celles des écrans branchés, supprime celles
    /// des écrans débranchés. La barre principale garde la fenêtre principale.
    fn sync_panels(&mut self) {
        let mut mons = monitors();
        if mons.is_empty() {
            return;
        }
        if !self.cfg.all_monitors {
            mons.truncate(1);
        }
        while self.panels.len() > mons.len() {
            let p = self.panels.pop().expect("barre en trop");
            let mut d = appbar_data(p.hwnd);
            // SAFETY: notre fenêtre secondaire, retirée du shell puis détruite.
            unsafe {
                SHAppBarMessage(ABM_REMOVE, &mut d);
                DestroyWindow(p.hwnd);
            }
        }
        while self.panels.len() < mons.len() {
            let hwnd = create_panel_window();
            if hwnd.is_null() {
                break;
            }
            self.panels.push(Panel {
                hwnd,
                monitor: 0,
                rect: Rect::default(),
                scale: 1.0,
                items: Vec::new(),
                wins: Vec::new(),
                hidden: false,
            });
        }
        for (p, (mon, rect)) in self.panels.iter_mut().zip(mons) {
            p.monitor = mon;
            p.rect = rect;
            p.scale = monitor_scale(mon);
            if !p.hidden {
                // SAFETY: notre fenêtre.
                unsafe { ShowWindow(p.hwnd, SW_SHOWNOACTIVATE) };
            }
        }
        if let Some(p) = self.panels.first() {
            self.scale = p.scale;
        }
    }

    /// Les écrans ont-ils changé depuis le dernier placement ?
    fn monitors_changed(&self) -> bool {
        let mut mons = monitors();
        if !self.cfg.all_monitors {
            mons.truncate(1);
        }
        mons.len() != self.panels.len()
            || mons
                .iter()
                .zip(&self.panels)
                .any(|((m, r), p)| *m != p.monitor || *r != p.rect)
    }

    /// (Re)place les barres : chacune réserve sa bande auprès du shell sur son écran,
    /// puis s'y positionne.
    fn dock(&mut self) {
        self.sync_panels();
        for p in &self.panels {
            let mut cfg = self.cfg.clone();
            cfg.thickness = (cfg.thickness as f32 * p.scale).round() as u32;
            cfg.margin = (cfg.margin as f32 * p.scale).round() as u32;
            let (bar, reserved) = bar_rect(p.rect, &cfg);
            let mut d = appbar_data(p.hwnd);
            d.uEdge = edge_code(cfg.edge);
            d.rc = RECT {
                left: reserved.left,
                top: reserved.top,
                right: reserved.right,
                bottom: reserved.bottom,
            };
            // SAFETY: structure locale ; la barre est enregistrée (ABM_NEW) avant.
            unsafe {
                SHAppBarMessage(ABM_QUERYPOS, &mut d);
                SHAppBarMessage(ABM_SETPOS, &mut d);
                SetWindowPos(
                    p.hwnd,
                    HWND_TOPMOST,
                    bar.left,
                    bar.top,
                    bar.width(),
                    bar.height(),
                    SWP_NOACTIVATE,
                );
                SetLayeredWindowAttributes(p.hwnd, 0, (self.cfg.opacity as u32 * 255 / 100) as u8, LWA_ALPHA);
                // Coins arrondis (Windows 11) : DWMWA_WINDOW_CORNER_PREFERENCE = 33.
                let pref: u32 = if self.cfg.rounded { 2 } else { 1 };
                DwmSetWindowAttribute(p.hwnd, 33, &pref as *const _ as *const c_void, 4);
            }
        }
        self.relayout();
        self.sync_desktop();
    }

    /// Barre d'une de nos fenêtres.
    fn panel(&self, hwnd: HWND) -> Option<&Panel> {
        self.panels.iter().find(|p| p.hwnd == hwnd)
    }

    /// Ce qui est sous le point (x, y) de la barre `hwnd` : le widget, et la fenêtre
    /// si c'est un bouton de fenêtre.
    fn hit(&self, hwnd: HWND, x: i32, y: i32) -> Option<(Widget, Option<HWND>)> {
        let p = self.panel(hwnd)?;
        let item = p.items.iter().find(|i| i.rect.contains(x, y))?;
        let win = item
            .index
            .and_then(|i| p.wins.get(i))
            .and_then(|w| self.windows.get(*w))
            .map(|w| w.hwnd);
        Some((item.widget, win))
    }

    /// Plein écran signalé par Windows : chaque barre ne s'efface que si le plein
    /// écran est sur son écran.
    fn fullscreen_changed(&mut self, notified: HWND, on: bool) {
        if !self.cfg.hide_in_fullscreen {
            return;
        }
        // SAFETY: lectures d'état.
        let fg = unsafe { GetForegroundWindow() };
        let fs_monitor = (!fg.is_null() && is_fullscreen(fg))
            // SAFETY: renvoie toujours un écran.
            .then(|| unsafe { MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST) } as isize);
        for p in &mut self.panels {
            let hide = match (on, fs_monitor) {
                (false, _) => false,
                (true, Some(m)) => m == p.monitor,
                // Plein écran non identifiable : la barre prévenue s'efface.
                (true, None) => p.hwnd == notified || p.hidden,
            };
            if hide != p.hidden {
                p.hidden = hide;
                // SAFETY: notre fenêtre.
                unsafe { ShowWindow(p.hwnd, if hide { SW_HIDE } else { SW_SHOWNOACTIVATE }) };
            }
        }
        let desk_hidden = self.panels.iter().any(|p| p.hidden);
        if desk_hidden != self.desk_hidden {
            self.desk_hidden = desk_hidden;
            for d in &self.desk {
                // SAFETY: nos widgets.
                unsafe { ShowWindow(d.hwnd, if desk_hidden { SW_HIDE } else { SW_SHOWNOACTIVATE }) };
            }
        }
    }

    /// Crée, place ou supprime les widgets du bureau selon la configuration.
    fn sync_desktop(&mut self) {
        let wanted = self.cfg.desktop_widgets.clone();
        // SAFETY: fenêtres créées et détruites par ce processus.
        unsafe {
            self.desk.retain(|d| {
                let keep = wanted.iter().any(|w| w.kind == d.kind);
                if !keep {
                    DestroyWindow(d.hwnd);
                }
                keep
            });
            let alpha = (self.cfg.desktop_opacity as u32 * 255 / 100) as u8;
            for w in &wanted {
                let (lw, lh) = w.kind.size();
                let (x, y, cw, ch) = (self.scaled_i(w.x), self.scaled_i(w.y), self.scaled(lw), self.scaled(lh));
                let hwnd = match self.desk.iter().find(|d| d.kind == w.kind) {
                    Some(d) => d.hwnd,
                    None => {
                        let class = wide(DESK_CLASS);
                        let title = wide("Prism Widget");
                        let h = CreateWindowExW(
                            WS_EX_TOOLWINDOW | WS_EX_LAYERED | WS_EX_NOACTIVATE,
                            class.as_ptr(),
                            title.as_ptr(),
                            WS_POPUP,
                            x,
                            y,
                            cw,
                            ch,
                            null_mut(),
                            null_mut(),
                            GetModuleHandleW(null()),
                            null(),
                        );
                        if h.is_null() {
                            continue;
                        }
                        let pref: u32 = 2;
                        DwmSetWindowAttribute(h, 33, &pref as *const _ as *const c_void, 4);
                        if !self.desk_hidden {
                            ShowWindow(h, SW_SHOWNOACTIVATE);
                        }
                        self.desk.push(DeskWin { hwnd: h, kind: w.kind });
                        h
                    }
                };
                SetLayeredWindowAttributes(hwnd, 0, alpha, LWA_ALPHA);
                SetWindowPos(hwnd, HWND_BOTTOM, x, y, cw, ch, SWP_NOACTIVATE);
            }
        }
    }

    fn scaled_i(&self, v: i32) -> i32 {
        (v as f32 * self.scale).round() as i32
    }

    /// Le widget déplacé à la souris : sa nouvelle position est enregistrée.
    fn desk_moved(&mut self, hwnd: HWND) {
        let Some(kind) = self.desk.iter().find(|d| d.hwnd == hwnd).map(|d| d.kind) else {
            return;
        };
        let mut r = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        // SAFETY: sortie locale.
        unsafe { GetWindowRect(hwnd, &mut r) };
        if let Some(w) = self.cfg.desktop_widgets.iter_mut().find(|w| w.kind == kind) {
            w.x = (r.left as f32 / self.scale).round() as i32;
            w.y = (r.top as f32 / self.scale).round() as i32;
        }
        if self.cfg.save().is_ok() {
            self.cfg_stamp = std::fs::metadata(BarConfig::path()).and_then(|m| m.modified()).ok();
        }
    }

    fn paint_desk(&self, hwnd: HWND, hdc: HDC) {
        let Some(kind) = self.desk.iter().find(|d| d.hwnd == hwnd).map(|d| d.kind) else {
            return;
        };
        let mut rc = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        // SAFETY: double tampon GDI local, objets créés puis détruits ici.
        unsafe {
            GetClientRect(hwnd, &mut rc);
            let (w, h) = (rc.right, rc.bottom);
            let mem = CreateCompatibleDC(hdc);
            let bmp = CreateCompatibleBitmap(hdc, w, h);
            let old = SelectObject(mem, bmp as HGDIOBJ);
            fill(mem, rc, colors().bg);
            fill(
                mem,
                RECT {
                    left: 0,
                    top: 0,
                    right: w,
                    bottom: self.scaled(3),
                },
                colors().accent,
            );
            SetBkMode(mem, TRANSPARENT as i32);
            let huge = make_font(self.scaled(52), 300);
            let big = make_font(self.scaled(26), 500);
            let font = make_font(self.scaled(14), 400);
            let small = make_font(self.scaled(12), 400);
            let pad = self.scaled(14);
            let inner = RECT {
                left: pad,
                top: pad,
                right: w - pad,
                bottom: h - pad,
            };
            match kind {
                DeskKind::Clock => {
                    let mut t = std::mem::zeroed();
                    GetLocalTime(&mut t);
                    let top = RECT {
                        bottom: inner.top + (inner.bottom - inner.top) * 2 / 3,
                        ..inner
                    };
                    let bottom = RECT {
                        top: top.bottom,
                        ..inner
                    };
                    text(
                        mem,
                        huge,
                        colors().text,
                        top,
                        &format!("{:02}:{:02}", t.wHour, t.wMinute),
                        DT_LEFT,
                    );
                    text(
                        mem,
                        font,
                        colors().muted,
                        bottom,
                        &date_fr(t.wDayOfWeek, t.wDay, t.wMonth, t.wYear),
                        DT_LEFT,
                    );
                }
                DeskKind::System => {
                    let rows: [(&str, Option<f32>, String); 4] = [
                        ("Processeur", Some(self.sample.cpu), format!("{:.0} %", self.sample.cpu)),
                        (
                            "Mémoire",
                            Some(self.sample.ram),
                            format!("{} / {}", gib(self.sample.ram_used), gib(self.sample.ram_total)),
                        ),
                        (
                            "GPU",
                            self.sample.gpu,
                            self.sample
                                .gpu
                                .map(|g| format!("{g:.0} %"))
                                .unwrap_or_else(|| "—".into()),
                        ),
                        (
                            "Réseau",
                            None,
                            format!(
                                "↓ {}  ↑ {}",
                                human_rate(self.sample.net_down),
                                human_rate(self.sample.net_up)
                            ),
                        ),
                    ];
                    let row_h = (inner.bottom - inner.top) / 4;
                    for (i, (label, value, shown)) in rows.iter().enumerate() {
                        let top = inner.top + row_h * i as i32;
                        let line = RECT {
                            top,
                            bottom: top + row_h / 2 + 4,
                            ..inner
                        };
                        text(mem, small, colors().muted, line, label, DT_LEFT);
                        text(mem, font, colors().text, line, shown, DT_RIGHT);
                        if let Some(v) = value {
                            let bar = RECT {
                                left: inner.left,
                                top: top + row_h / 2 + 6,
                                right: inner.right,
                                bottom: top + row_h / 2 + 10,
                            };
                            fill(mem, bar, colors().item);
                            let filled = RECT {
                                right: bar.left + ((bar.right - bar.left) as f32 * v / 100.0) as i32,
                                ..bar
                            };
                            fill(mem, filled, if *v > 85.0 { colors().warn } else { colors().accent });
                        }
                    }
                }
                DeskKind::Cpu | DeskKind::Ram | DeskKind::Gpu => {
                    let (label, value, hist) = match kind {
                        DeskKind::Cpu => ("Processeur", Some(self.sample.cpu), &self.cpu),
                        DeskKind::Ram => ("Mémoire", Some(self.sample.ram), &self.ram),
                        _ => ("GPU", self.sample.gpu, &self.gpu),
                    };
                    let head = RECT {
                        bottom: inner.top + self.scaled(34),
                        ..inner
                    };
                    text(mem, small, colors().muted, head, label, DT_LEFT);
                    let shown = value.map(|v| format!("{v:.0} %")).unwrap_or_else(|| "—".into());
                    text(mem, big, colors().text, head, &shown, DT_RIGHT);
                    let graph = RECT {
                        top: head.bottom + self.scaled(6),
                        ..inner
                    };
                    graph_line(mem, graph, hist.values(), colors().accent);
                }
                DeskKind::Network => {
                    let half = (inner.bottom - inner.top) / 2;
                    text(
                        mem,
                        small,
                        colors().muted,
                        RECT {
                            bottom: inner.top + half / 2,
                            ..inner
                        },
                        "Réseau",
                        DT_LEFT,
                    );
                    let down = RECT {
                        top: inner.top + half / 2,
                        bottom: inner.top + half + half / 3,
                        ..inner
                    };
                    let up = RECT {
                        top: down.bottom,
                        ..inner
                    };
                    text(
                        mem,
                        big,
                        colors().ok,
                        down,
                        &format!("↓ {}", human_rate(self.sample.net_down)),
                        DT_LEFT,
                    );
                    text(
                        mem,
                        font,
                        colors().accent,
                        up,
                        &format!("↑ {}", human_rate(self.sample.net_up)),
                        DT_LEFT,
                    );
                }
            }
            BitBlt(hdc, 0, 0, w, h, mem, 0, 0, SRCCOPY);
            SelectObject(mem, old);
            for f in [huge, big, font, small] {
                DeleteObject(f as HGDIOBJ);
            }
            DeleteObject(bmp as HGDIOBJ);
            DeleteDC(mem);
        }
    }

    fn relayout(&mut self) {
        // Écran de chaque fenêtre, en numéro de barre (hors barres : barre principale).
        let window_monitor: Vec<usize> = self
            .windows
            .iter()
            .map(|w| {
                // SAFETY: renvoie toujours un écran.
                let m = unsafe { MonitorFromWindow(w.hwnd, MONITOR_DEFAULTTONEAREST) } as isize;
                self.panels.iter().position(|p| p.monitor == m).unwrap_or(usize::MAX)
            })
            .collect();
        let panel_monitor: Vec<usize> = (0..self.panels.len()).collect();
        let lists = prism_core::bar::panel_windows(&window_monitor, &panel_monitor, self.cfg.windows_per_monitor);
        for (p, wins) in self.panels.iter_mut().zip(lists) {
            let (w, h) = client_size(p.hwnd);
            p.items = layout(&self.cfg, w, h, wins.len());
            p.wins = wins;
        }
    }

    fn reload_config_if_changed(&mut self) {
        let stamp = std::fs::metadata(BarConfig::path()).and_then(|m| m.modified()).ok();
        if stamp != self.cfg_stamp {
            self.cfg_stamp = stamp;
            let new = BarConfig::load();
            set_colors(&new.theme);
            if self.cfg.tiling.enabled && !new.tiling.enabled {
                self.tiler.restore_all();
            }
            if new.hide_windows_taskbar != self.cfg.hide_windows_taskbar {
                if new.hide_windows_taskbar {
                    hide_taskbar();
                } else {
                    show_taskbar();
                }
            }
            self.cfg = new;
            self.dock();
        }
    }

    fn tick(&mut self) {
        self.reload_config_if_changed();
        for l in self.tiler.log.drain(..) {
            fx_log(|| format!("tuiles : {l}"));
        }
        if self.monitors_changed() {
            self.dock();
        }
        FX_DEBUG.store(
            prism_core::paths::data_dir().join("fx-debug.log").exists(),
            std::sync::atomic::Ordering::Relaxed,
        );
        self.sample = self.metrics.sample();
        self.cpu.push(self.sample.cpu);
        self.ram.push(self.sample.ram);
        if let Some(g) = self.sample.gpu {
            self.gpu.push(g);
        }
        self.game = Etat::load().is_some_and(|e| !e.game.is_empty());
        self.windows = list_windows(self.hwnd);
        self.apply_opacity_rules();
        self.retile();
        self.relayout();
        // SAFETY: invalidation de nos propres fenêtres.
        unsafe {
            for p in &self.panels {
                InvalidateRect(p.hwnd, null(), 0);
            }
            for d in &self.desk {
                InvalidateRect(d.hwnd, null(), 0);
            }
        }
    }

    /// Transparence par appli. Jamais sur un jeu, une fenêtre plein écran, ni une
    /// fenêtre qui gère déjà sa propre transparence ; tout est remis sinon.
    fn apply_opacity_rules(&mut self) {
        let mut seen = std::collections::HashSet::new();
        for w in &self.windows {
            let key = w.hwnd as isize;
            seen.insert(key);
            let path = crate::win::process_path(w.pid);
            let exe = path
                .as_deref()
                .and_then(|p| p.rsplit('\\').next())
                .unwrap_or_default()
                .to_string();
            let wanted = self.cfg.rule_for(&exe).filter(|_| {
                !prism_core::classify::is_game_process(&exe, path.as_deref(), &self.game_cfg) && !is_fullscreen(w.hwnd)
            });
            // SAFETY: fenêtres d'autres processus ; seuls le style étendu et l'opacité
            // de calque sont modifiés, avec retour à l'état d'origine.
            unsafe {
                match (wanted, self.translucent.get(&key).copied()) {
                    (Some(op), None) => {
                        let ex = GetWindowLongPtrW(w.hwnd, GWL_EXSTYLE);
                        if ex as u32 & WS_EX_LAYERED != 0 {
                            continue; // l'appli gère déjà sa transparence
                        }
                        SetWindowLongPtrW(w.hwnd, GWL_EXSTYLE, ex | WS_EX_LAYERED as isize);
                        SetLayeredWindowAttributes(w.hwnd, 0, (op as u32 * 255 / 100) as u8, LWA_ALPHA);
                        self.translucent.insert(key, (ex, op));
                    }
                    (Some(op), Some((ex, applied))) if op != applied => {
                        SetLayeredWindowAttributes(w.hwnd, 0, (op as u32 * 255 / 100) as u8, LWA_ALPHA);
                        self.translucent.insert(key, (ex, op));
                    }
                    (None, Some((ex, _))) => {
                        SetWindowLongPtrW(w.hwnd, GWL_EXSTYLE, ex);
                        self.translucent.remove(&key);
                    }
                    _ => {}
                }
            }
        }
        // Fenêtres fermées : rien à remettre.
        self.translucent.retain(|k, _| seen.contains(k));
    }

    /// Remet toutes les fenêtres rendues transparentes (arrêt de la barre).
    fn restore_opacity(&mut self) {
        for (k, (ex, _)) in self.translucent.drain() {
            // SAFETY: remise du style étendu d'origine ; sans effet si la fenêtre n'existe plus.
            unsafe { SetWindowLongPtrW(k as HWND, GWL_EXSTYLE, ex) };
        }
    }

    fn click(&mut self, panel: HWND, x: i32, y: i32) {
        let Some((widget, win)) = self.hit(panel, x, y) else {
            return;
        };
        match widget {
            Widget::Start => press_win_key(),
            Widget::Windows => {
                if let Some(h) = win {
                    if !self.fx_click(h, "clic barre") {
                        activate(h);
                    }
                }
            }
            Widget::GameMode | Widget::Cpu | Widget::Ram | Widget::Gpu => open_prism_ui(),
            _ => {}
        }
    }

    // --- Effets ---------------------------------------------------------------

    /// La fenêtre peut-elle recevoir un effet ? (jamais un jeu, ni en plein écran,
    /// ni pendant le Mode Jeu, ni une petite fenêtre outil.)
    fn fx_allowed(&self, hwnd: HWND, need_visible: bool) -> bool {
        if !self.cfg.fx.enabled || self.game || hwnd.is_null() || hwnd == self.hwnd {
            return false;
        }
        // SAFETY: lectures d'attributs d'une fenêtre.
        unsafe {
            if need_visible && IsWindowVisible(hwnd) == 0 {
                return false;
            }
            if !GetWindow(hwnd, GW_OWNER).is_null() {
                return false;
            }
            let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
            if style & WS_CAPTION != WS_CAPTION || ex & WS_EX_TOOLWINDOW != 0 {
                return false;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid == std::process::id() {
                return false;
            }
            let path = crate::win::process_path(pid);
            let exe = path
                .as_deref()
                .and_then(|p| p.rsplit('\\').next())
                .unwrap_or_default()
                .to_string();
            if prism_core::classify::window_untouchable(&exe, path.as_deref(), &self.game_cfg) {
                return false;
            }
        }
        // Une fenêtre réduite est rangée hors écran par Windows (160x28) : sa
        // géométrie utile est celle de sa dernière copie, pas celle-là.
        // SAFETY: lecture d'état.
        if unsafe { IsIconic(hwnd) } != 0 {
            return self.snaps.contains_key(&(hwnd as isize));
        }
        match crate::fx_overlay::visible_rect(hwnd) {
            Some(r) => r.width() >= 200 && r.height() >= 120 && !is_fullscreen(hwnd),
            None => false,
        }
    }

    /// Bouton de la fenêtre dans la barre, en coordonnées écran (cible du génie).
    fn target_for(&self, hwnd: HWND) -> Rect {
        let mut br = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        // La barre qui montre la fenêtre, de préférence celle de son écran.
        let idx = self.windows.iter().position(|w| w.hwnd == hwnd);
        // SAFETY: renvoie toujours un écran.
        let mon = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) } as isize;
        let shows = |p: &&Panel| idx.is_some_and(|i| p.wins.contains(&i));
        let panel = self
            .panels
            .iter()
            .filter(shows)
            .find(|p| p.monitor == mon)
            .or_else(|| self.panels.iter().find(shows))
            .or_else(|| self.panels.iter().find(|p| p.monitor == mon))
            .or(self.panels.first());
        let Some(panel) = panel else {
            return Rect::default();
        };
        // SAFETY: sortie locale.
        unsafe { GetWindowRect(panel.hwnd, &mut br) };
        let local = idx.and_then(|i| panel.wins.iter().position(|w| *w == i));
        let item = panel
            .items
            .iter()
            .find(|p| p.widget == Widget::Windows && p.index.is_some() && p.index == local);
        match item {
            Some(p) => Rect {
                left: br.left + p.rect.left,
                top: br.top + p.rect.top,
                right: br.left + p.rect.right,
                bottom: br.top + p.rect.bottom,
            },
            None => {
                let (cx, cy) = ((br.left + br.right) / 2, (br.top + br.bottom) / 2);
                Rect {
                    left: cx - 20,
                    top: cy - 10,
                    right: cx + 20,
                    bottom: cy + 10,
                }
            }
        }
    }

    /// Joue un effet sur une copie de fenêtre et enregistre sa mesure.
    #[allow(clippy::too_many_arguments)]
    fn play_effect(
        &mut self,
        effect: prism_core::fx::Effect,
        kind: FxKind,
        snap: &crate::fx_overlay::Snap,
        target: Rect,
        trigger: Instant,
        name: &str,
        after_first: impl FnOnce(),
    ) {
        use prism_core::fx::{Animation, Effect};
        if effect == Effect::None {
            after_first();
            return;
        }
        let Some(overlay) = self.overlay.as_mut() else {
            after_first();
            return;
        };
        let w = snap.rect;
        // Zone de la couche : fenêtre + cible + marge pour le dépassement de la gélatine.
        let (mx, my) = (w.width() / 6 + 40, w.height() / 6 + 40);
        // SAFETY: lectures de métriques système.
        let (vx, vy, vw, vh) = unsafe {
            (
                GetSystemMetrics(SM_XVIRTUALSCREEN),
                GetSystemMetrics(SM_YVIRTUALSCREEN),
                GetSystemMetrics(SM_CXVIRTUALSCREEN),
                GetSystemMetrics(SM_CYVIRTUALSCREEN),
            )
        };
        let left = (w.left - mx).min(target.left).max(vx);
        let top = (w.top - my).min(target.top).max(vy);
        let right = (w.right + mx).max(target.right).min(vx + vw);
        let bottom = (w.bottom + my).max(target.bottom).min(vy + vh);
        if right <= left || bottom <= top {
            after_first();
            return;
        }
        let shift = |r: Rect| Rect {
            left: r.left - left,
            top: r.top - top,
            right: r.right - left,
            bottom: r.bottom - top,
        };
        // Génie et zoom se jouent à l'envers pour une apparition, la gélatine pour une disparition.
        let reverse = (kind == FxKind::Appear) ^ (effect == Effect::Jelly);
        let anim = Animation {
            effect,
            source: snap.img.clone(),
            window: shift(w),
            target: shift(target),
            edge: self.cfg.edge,
            reverse,
            intensity: self.cfg.fx.intensity as f32 / 100.0,
        };
        let duration = Duration::from_millis(self.cfg.fx.duration_ms as u64);
        let stat = overlay.play(
            &anim,
            (left, top),
            ((right - left) as u32, (bottom - top) as u32),
            duration,
            trigger,
            name,
            snap.capture_ms,
            after_first,
        );
        prism_core::fx::push_stat(&mut self.stats, stat);
        let _ = prism_core::fx::save_stats(&self.stats);
    }

    /// Appui sur un bouton de fenêtre : on copie la fenêtre pendant que le doigt
    /// est encore sur le bouton (un clic dure environ 100 ms), l'effet de réduction
    /// démarre alors sans attente au relâchement.
    fn prepare_click(&mut self, panel: HWND, x: i32, y: i32) {
        let Some((Widget::Windows, Some(h))) = self.hit(panel, x, y) else {
            return;
        };
        // SAFETY: lectures d'état.
        let ready = unsafe { GetForegroundWindow() == h && IsIconic(h) == 0 };
        if ready && self.cfg.fx.minimize != prism_core::fx::Effect::None && self.fx_allowed(h, true) {
            if let Some(s) = crate::fx_overlay::capture(h) {
                self.snaps.insert(h as isize, s);
            }
        }
    }

    /// Clic sur une fenêtre de la barre : réduction ou restauration avec effet.
    /// `false` si aucun effet ne s'applique (le clic active alors la fenêtre).
    fn fx_click(&mut self, h: HWND, name: &str) -> bool {
        if !self.fx_allowed(h, false) {
            return false;
        }
        let key = h as isize;
        let trigger = Instant::now();
        // SAFETY: lectures d'état d'une fenêtre.
        let (iconic, foreground) = unsafe { (IsIconic(h) != 0, GetForegroundWindow() == h) };
        let fx = self.cfg.fx.clone();
        if foreground && !iconic && fx.minimize != prism_core::fx::Effect::None {
            // Copie faite à l'appui du bouton (voir `prepare_click`) si elle est fraîche,
            // sinon maintenant.
            let fresh = self
                .snaps
                .remove(&key)
                .filter(|s| s.at.elapsed() < Duration::from_millis(1500));
            let Some(snap) = fresh.or_else(|| crate::fx_overlay::capture(h)) else {
                return false;
            };
            self.skip.insert((key, EVENT_SYSTEM_MINIMIZESTART_ID), Instant::now());
            let target = self.target_for(h);
            // La fenêtre est réduite dès que la couche la recouvre.
            self.play_effect(
                fx.minimize,
                FxKind::Disappear,
                &snap,
                target,
                trigger,
                name,
                || unsafe {
                    ShowWindow(h, SW_MINIMIZE);
                },
            );
            self.snaps.insert(key, snap);
            return true;
        }
        if iconic && fx.restore != prism_core::fx::Effect::None {
            let Some(snap) = self.snaps.remove(&key) else {
                return false;
            };
            self.skip.insert((key, EVENT_SYSTEM_MINIMIZEEND_ID), Instant::now());
            let target = self.target_for(h);
            let hidden = crate::fx_overlay::hide_temp(h);
            // Restaurée invisible sous la couche, rendue visible à la dernière image.
            self.play_effect(fx.restore, FxKind::Appear, &snap, target, trigger, name, || unsafe {
                ShowWindow(h, SW_RESTORE);
            });
            if let Some(ex) = hidden {
                crate::fx_overlay::unhide(h, ex);
            }
            activate(h);
            self.snaps.insert(key, snap);
            return true;
        }
        false
    }

    /// Début d'un déplacement : copie, fenêtre rendue invisible, gélatine lancée.
    fn drag_start(&mut self, h: HWND, at: Instant) {
        // SAFETY: lectures d'état.
        let (maximized, mut cursor) = unsafe { (IsZoomed(h) != 0, POINT { x: 0, y: 0 }) };
        if maximized {
            return; // la déplacer la restaure : sa taille change
        }
        // SAFETY: sortie locale.
        unsafe { GetCursorPos(&mut cursor) };
        let Some(r) = crate::fx_overlay::visible_rect(h) else {
            return;
        };
        // Un redimensionnement commence aussi par cet événement : poignées sur les bords.
        let grip = (6.0 * self.scale).round() as i32;
        let on_border = cursor.x < r.left + grip
            || cursor.x >= r.right - grip
            || cursor.y < r.top + grip / 2
            || cursor.y >= r.bottom - grip;
        let keyboard = !r.contains(cursor.x, cursor.y);
        fx_log(|| {
            format!(
                "drag_start rect {r:?} cursor {},{} border {on_border} keyboard {keyboard}",
                cursor.x, cursor.y
            )
        });
        if on_border && !keyboard {
            return;
        }
        let Some(snap) = crate::fx_overlay::capture(h) else {
            return;
        };
        let Some(ex) = crate::fx_overlay::hide_temp(h) else {
            return; // la fenêtre gère déjà sa transparence : on n'y touche pas
        };
        let grab = if keyboard {
            (snap.rect.width() as f32 / 2.0, 0.0)
        } else {
            ((cursor.x - snap.rect.left) as f32, (cursor.y - snap.rect.top) as f32)
        };
        let wob = prism_core::wobbly::Wobbly::new(
            snap.img.width,
            snap.img.height,
            (snap.rect.left as f32, snap.rect.top as f32),
            grab,
            self.cfg.fx.intensity as f32 / 100.0,
        );
        let margin = wob.margin();
        let frame = prism_core::fx::Image::new(snap.img.width + 2 * margin as u32, snap.img.height + 2 * margin as u32);
        self.drag = Some(DragFx {
            hwnd: h,
            ex,
            snap,
            wob,
            frame,
            margin,
            last: Instant::now(),
            started: Instant::now(),
            released: None,
            // SAFETY: lecture du thread propriétaire.
            thread: unsafe { GetWindowThreadProcessId(h, null_mut()) },
            meter: crate::fx_overlay::Meter::new(at),
            pos: (0, 0),
        });
        self.drag_frame();
    }

    /// Une image de la gélatine, puis la suivante est demandée par message.
    fn drag_frame(&mut self) {
        let Some(d) = self.drag.as_mut() else { return };
        let Some(overlay) = self.overlay.as_mut() else {
            self.drag_end();
            return;
        };
        let w0 = Instant::now();
        // SAFETY: lecture d'état ; la fenêtre peut avoir été fermée pendant le déplacement.
        let alive = unsafe { IsWindow(d.hwnd) != 0 };
        let rect = alive.then(|| crate::fx_overlay::visible_rect(d.hwnd)).flatten();
        let Some(r) = rect else {
            fx_log(|| "drag: window gone".into());
            self.drag_end();
            return;
        };
        // Taille changée (redimensionnement, ancrage, agrandissement) : la vraie fenêtre
        // reprend la main tout de suite.
        if (r.width() - d.snap.rect.width()).abs() > 2 || (r.height() - d.snap.rect.height()).abs() > 2 {
            fx_log(|| format!("drag: size changed {r:?}"));
            // Ancrage sur un bord en fin de déplacement : la gélatine glisse vers le
            // nouveau cadre sans que la vraie fenêtre réapparaisse entre les deux.
            if self.cfg.fx.maximize != prism_core::fx_effects::MorphEffect::None && d.meter.frames > 0 {
                let d = self.drag.take().expect("déplacement en cours");
                let from = Rect {
                    left: d.pos.0,
                    top: d.pos.1,
                    right: d.pos.0 + d.snap.rect.width(),
                    bottom: d.pos.1 + d.snap.rect.height(),
                };
                self.geo.insert(d.hwnd as isize, r);
                self.play_morph(d.hwnd, d.snap.img, from, r, Some(d.ex), Instant::now(), "ancrage");
                return;
            }
            self.drag_end();
            return;
        }
        // La fin du déplacement est lue sur le thread de la fenêtre : l'événement de fin
        // n'arrive pas toujours (vu en VM).
        if d.released.is_none() && d.meter.frames > 0 && !in_move_size(d.thread) {
            d.released = Some(Instant::now());
        }
        let dt = d.last.elapsed().as_secs_f32();
        d.last = Instant::now();
        d.wob.step((r.left as f32, r.top as f32), dt);
        d.pos = (r.left, r.top);
        let m = d.margin as f32;
        d.wob.render(&d.snap.img, (m, m), &mut d.frame);
        overlay.show_frame(&d.frame, r.left - d.margin, r.top - d.margin);
        if matches!(d.meter.frames, 6 | 12 | 18 | 24) {
            fx_dump(&d.frame, &format!("drag-{:02}", d.meter.frames));
        }
        if d.meter.frames == 0 {
            overlay.raise();
        }
        d.meter.frame(w0);
        let done = match d.released {
            Some(t) => d.wob.settled() || t.elapsed() > Duration::from_millis(1500),
            // Filet de sécurité si la fin du déplacement n'arrive jamais.
            None => d.started.elapsed() > Duration::from_secs(120),
        };
        if d.meter.frames % 30 == 1 || done {
            fx_log(|| {
                format!(
                    "drag_frame #{} at {},{} released {:?} settled {}",
                    d.meter.frames,
                    r.left,
                    r.top,
                    d.released.map(|t| t.elapsed()),
                    d.wob.settled()
                )
            });
        }
        if done {
            self.drag_end();
        } else {
            // SAFETY: message à notre propre fenêtre.
            unsafe { PostMessageW(self.hwnd, WM_FX_DRAG, 0, 0) };
        }
    }

    /// Suit l'appli au premier plan : son cadre, et l'abonnement aux changements de
    /// position de ses fenêtres (seulement si la glisse est activée).
    fn follow_foreground(&mut self, h: HWND) {
        use windows_sys::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent};
        // SAFETY: lecture d'état.
        if unsafe { IsIconic(h) } == 0 {
            if let Some(r) = crate::fx_overlay::visible_rect(h) {
                self.geo.insert(h as isize, r);
            }
        }
        if self.geo.len() > 64 {
            // SAFETY: lecture d'état.
            self.geo.retain(|k, _| unsafe { IsWindow(*k as HWND) } != 0);
        }
        let want = self.cfg.fx.enabled && self.cfg.fx.maximize != prism_core::fx_effects::MorphEffect::None;
        let mut pid = 0u32;
        // SAFETY: sortie locale.
        unsafe { GetWindowThreadProcessId(h, &mut pid) };
        if want && self.loc_hook.is_some_and(|(_, p)| p == pid) {
            return;
        }
        if let Some((hook, _)) = self.loc_hook.take() {
            // SAFETY: abonnement créé ci-dessous.
            unsafe { UnhookWinEvent(hook as _) };
        }
        if want && pid != 0 && pid != std::process::id() {
            // SAFETY: rappel hors processus, limité à ce processus.
            let hook = unsafe {
                SetWinEventHook(
                    EVENT_OBJECT_LOCATIONCHANGE,
                    EVENT_OBJECT_LOCATIONCHANGE,
                    null_mut(),
                    Some(on_object_show),
                    pid,
                    0,
                    WINEVENT_OUTOFCONTEXT,
                )
            };
            if !hook.is_null() {
                self.loc_hook = Some((hook as isize, pid));
            }
        }
    }

    /// Une fenêtre a changé de place ou de taille. Si sa taille a changé d'un coup
    /// (agrandir, ancrer avec Win+flèche, revenir à la taille normale), glisse.
    fn on_location(&mut self, h: HWND, at: Instant) {
        let key = h as isize;
        if self.drag.as_ref().is_some_and(|d| d.hwnd == h) {
            return;
        }
        // SAFETY: lecture d'état.
        if unsafe { IsIconic(h) } != 0 {
            return;
        }
        let Some(new) = crate::fx_overlay::visible_rect(h) else {
            return;
        };
        let Some(old) = self.geo.insert(key, new) else {
            return;
        };
        if self.tiler.recently_moved(h) {
            return; // placée par les tuiles : pas d'animation « agrandir »
        }
        let resized = (new.width() - old.width()).abs() > 8 || (new.height() - old.height()).abs() > 8;
        if !resized || at.elapsed() > Duration::from_millis(250) || old.width() < 200 || old.height() < 120 {
            return;
        }
        // SAFETY: lecture du thread propriétaire.
        let thread = unsafe { GetWindowThreadProcessId(h, null_mut()) };
        if in_move_size(thread) {
            return; // redimensionnement à la souris : la fenêtre suit déjà la main
        }
        if self.cfg.fx.maximize == prism_core::fx_effects::MorphEffect::None || !self.fx_allowed(h, true) {
            return;
        }
        // Il faut une copie de l'ancien état, à l'ancienne taille.
        let Some(snap) = self.snaps.remove(&key) else {
            return;
        };
        if (snap.rect.width() - old.width()).abs() > 3 || (snap.rect.height() - old.height()).abs() > 3 {
            return;
        }
        fx_log(|| format!("morph {old:?} -> {new:?}"));
        self.play_morph(h, snap.img, old, new, None, at, "agrandir / ancrer");
    }

    /// Glisse de `from` vers `to` (coordonnées écran) avec la copie `img` ; la vraie
    /// fenêtre, cachée, réapparaît en fondu dessous. `hidden` : déjà cachée (style d'origine).
    #[allow(clippy::too_many_arguments)]
    fn play_morph(
        &mut self,
        h: HWND,
        img: prism_core::fx::Image,
        from: Rect,
        to: Rect,
        hidden: Option<isize>,
        trigger: Instant,
        name: &str,
    ) {
        let Some(ex) = hidden.or_else(|| crate::fx_overlay::hide_temp(h)) else {
            return; // la fenêtre gère déjà sa transparence : on n'y touche pas
        };
        let fx = self.cfg.fx.clone();
        let mut morph = prism_core::fx_effects::Morph {
            effect: fx.maximize,
            source: img,
            from,
            to,
            intensity: fx.intensity as f32 / 100.0,
        };
        // SAFETY: lectures de métriques système.
        let (vx, vy, vw, vh) = unsafe {
            (
                GetSystemMetrics(SM_XVIRTUALSCREEN),
                GetSystemMetrics(SM_YVIRTUALSCREEN),
                GetSystemMetrics(SM_CXVIRTUALSCREEN),
                GetSystemMetrics(SM_CYVIRTUALSCREEN),
            )
        };
        let b = morph.bounds();
        let (left, top) = (b.left.max(vx), b.top.max(vy));
        let (right, bottom) = (b.right.min(vx + vw), b.bottom.min(vy + vh));
        let size = ((right - left).max(1) as u32, (bottom - top).max(1) as u32);
        let shift = |r: Rect| Rect {
            left: r.left - left,
            top: r.top - top,
            right: r.right - left,
            bottom: r.bottom - top,
        };
        morph.from = shift(from);
        morph.to = shift(to);
        // Un peu plus court que les autres effets : on attend la fenêtre agrandie.
        let duration = Duration::from_millis(fx.duration_ms as u64 * 4 / 5);
        let mut shown = 0u32;
        if let Some(mut overlay) = self.overlay.take() {
            let meter = overlay.run(
                (left, top),
                size,
                duration,
                trigger,
                |t, frame| {
                    morph.render(t, frame);
                    shown += 1;
                    if matches!(shown, 3 | 6 | 9 | 12) {
                        fx_dump(frame, &format!("morph-{shown:02}"));
                    }
                },
                |_, t| crate::fx_overlay::set_alpha(h, morph.window_alpha(t)),
            );
            self.overlay = Some(overlay);
            let stat = meter.stat(
                &format!("{:?}", fx.maximize),
                name,
                (morph.source.width, morph.source.height),
                0.0,
            );
            prism_core::fx::push_stat(&mut self.stats, stat);
            let _ = prism_core::fx::save_stats(&self.stats);
        }
        crate::fx_overlay::unhide(h, ex);
        // Nouvelle copie à la nouvelle taille (prochaine glisse, réduction…), hors du
        // chemin critique : l'animation est finie.
        if let Some(s) = crate::fx_overlay::capture(h) {
            self.snaps.insert(h as isize, s);
        }
    }

    /// Fin : la vraie fenêtre réapparaît à sa place, la couche disparaît, mesure gardée.
    fn drag_end(&mut self) {
        let Some(d) = self.drag.take() else { return };
        fx_log(|| format!("drag_end after {} frames", d.meter.frames));
        crate::fx_overlay::unhide(d.hwnd, d.ex);
        if let Some(o) = self.overlay.as_ref() {
            o.hide();
        }
        if d.meter.frames > 0 {
            let stat = d.meter.stat(
                "Wobbly",
                "déplacement",
                (d.snap.img.width, d.snap.img.height),
                d.snap.capture_ms,
            );
            prism_core::fx::push_stat(&mut self.stats, stat);
            let _ = prism_core::fx::save_stats(&self.stats);
        }
        self.snaps.insert(d.hwnd as isize, d.snap);
    }

    /// Traite les événements de fenêtres en attente.
    fn process_fx(&mut self) {
        let events: Vec<(u32, isize, Instant)> = FX_QUEUE.with(|q| std::mem::take(&mut *q.borrow_mut()));
        if !self.cfg.fx.enabled {
            return;
        }
        let structural = events.iter().any(|(ev, _, _)| {
            matches!(
                *ev,
                EVENT_OBJECT_SHOW_ID
                    | EVENT_OBJECT_HIDE_ID
                    | EVENT_SYSTEM_MINIMIZESTART_ID
                    | EVENT_SYSTEM_MINIMIZEEND_ID
            )
        });
        if structural && self.cfg.tiling.enabled {
            // Fenêtre ouverte, fermée, réduite ou rendue : les tuiles se réorganisent
            // tout de suite (sans attendre la seconde suivante).
            self.windows = list_windows(self.hwnd);
            self.retile();
        }
        for (ev, key, at) in events {
            let h = key as HWND;
            fx_log(|| {
                format!(
                    "event {ev:#x} hwnd {key:#x} age {:?} allowed {}",
                    at.elapsed(),
                    self.fx_allowed(h, true)
                )
            });
            if let Some(t) = self.skip.remove(&(key, ev)) {
                if t.elapsed() < Duration::from_secs(10) {
                    continue; // Prism l'a provoqué lui-même : déjà animé
                }
            }
            // Une animation en retard est pire que pas d'animation (vu en VM : 501 ms).
            if ev == EVENT_SYSTEM_MOVESIZEEND_ID {
                // Jamais abandonné : c'est lui qui rend la vraie fenêtre.
                if let Some(d) = self.drag.as_mut().filter(|d| d.hwnd == h) {
                    d.released.get_or_insert(at);
                }
                if self.cfg.tiling.enabled {
                    self.tile_drop(h);
                }
                continue;
            }
            if ev == EVENT_OBJECT_LOCATIONCHANGE_ID {
                self.on_location(h, at);
                continue;
            }
            if ev != EVENT_SYSTEM_FOREGROUND_ID && at.elapsed() > Duration::from_millis(250) {
                continue;
            }
            let fx = self.cfg.fx.clone();
            if ev == EVENT_SYSTEM_FOREGROUND_ID {
                self.follow_foreground(h);
            }
            match ev {
                EVENT_SYSTEM_FOREGROUND_ID if self.fx_allowed(h, true) => {
                    if let Some(s) = crate::fx_overlay::capture(h) {
                        self.snaps.insert(key, s);
                    }
                    if self.snaps.len() > 16 {
                        if let Some(oldest) = self.snaps.iter().min_by_key(|(_, s)| s.at).map(|(k, _)| *k) {
                            self.snaps.remove(&oldest);
                        }
                    }
                }
                EVENT_SYSTEM_MINIMIZESTART_ID if self.fx_allowed(h, false) => {
                    // Copie fraîche si la fenêtre est encore affichée (celle prise au
                    // focus peut dater de plusieurs minutes), sinon la dernière.
                    let fresh = crate::fx_overlay::capture(h);
                    if let Some(snap) = fresh.or_else(|| self.snaps.remove(&key)) {
                        let target = self.target_for(h);
                        self.play_effect(fx.minimize, FxKind::Disappear, &snap, target, at, "réduction", || {});
                        self.snaps.insert(key, snap);
                    }
                }
                EVENT_SYSTEM_MINIMIZEEND_ID if self.fx_allowed(h, false) => {
                    if let Some(snap) = self.snaps.remove(&key) {
                        if let Some(ex) = crate::fx_overlay::hide_temp(h) {
                            let target = self.target_for(h);
                            self.play_effect(fx.restore, FxKind::Appear, &snap, target, at, "restauration", || {});
                            crate::fx_overlay::unhide(h, ex);
                        }
                        self.snaps.insert(key, snap);
                    }
                }
                EVENT_SYSTEM_MOVESIZESTART_ID if fx.drag && self.drag.is_none() && self.fx_allowed(h, true) => {
                    self.drag_start(h, at);
                }
                EVENT_OBJECT_SHOW_ID
                    if fx.open != prism_core::fx::Effect::None
                        && !self.snaps.contains_key(&key)
                        && self.fx_allowed(h, true) =>
                {
                    if let Some(ex) = crate::fx_overlay::hide_temp(h) {
                        if let Some(snap) = crate::fx_overlay::capture(h) {
                            let target = self.target_for(h);
                            self.play_effect(fx.open, FxKind::Appear, &snap, target, at, "ouverture", || {});
                            self.snaps.insert(key, snap);
                        }
                        crate::fx_overlay::unhide(h, ex);
                    }
                }
                EVENT_OBJECT_HIDE_ID if fx.close != prism_core::fx::Effect::None && !self.game => {
                    // SAFETY: lectures d'état ; la fenêtre peut déjà être détruite.
                    let gone = unsafe { IsWindow(h) == 0 || (IsWindowVisible(h) == 0 && IsIconic(h) == 0) };
                    if gone {
                        if let Some(snap) = self.snaps.remove(&key) {
                            let target = self.target_for(h);
                            self.play_effect(fx.close, FxKind::Disappear, &snap, target, at, "fermeture", || {});
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Fenêtres à mettre en tuiles : comme la barre des tâches, sans jeu, plein écran,
    /// exclusion ni fenêtre de Prism.
    fn tile_candidates(&self) -> Vec<HWND> {
        self.windows
            .iter()
            .filter(|w| w.pid != std::process::id())
            .map(|w| w.hwnd)
            .filter(|h| crate::tiler::tileable_shape(*h) && !is_fullscreen(*h))
            .filter(|h| {
                let mut pid = 0u32;
                // SAFETY: sortie locale.
                unsafe { GetWindowThreadProcessId(*h, &mut pid) };
                let path = crate::win::process_path(pid);
                let exe = path
                    .as_deref()
                    .and_then(|p| p.rsplit('\\').next())
                    .unwrap_or_default()
                    .to_string();
                !prism_core::classify::window_untouchable(&exe, path.as_deref(), &self.game_cfg)
                    && !self.cfg.tiling.excluded(&exe)
            })
            .collect()
    }

    /// Met les fenêtres en tuiles (si activé et hors Mode Jeu : rien ne bouge pendant
    /// une partie).
    fn retile(&mut self) {
        if !self.cfg.tiling.enabled || self.game {
            return;
        }
        let wins = self.tile_candidates();
        fx_log(|| {
            let names: Vec<String> = wins
                .iter()
                .map(|h| {
                    let t = self
                        .windows
                        .iter()
                        .find(|w| w.hwnd == *h)
                        .map(|w| w.title.clone())
                        .unwrap_or_default();
                    format!("{:#x} {t}", *h as isize)
                })
                .collect();
            format!("tuiles : candidates {names:?}")
        });
        let n = self.tiler.apply(&wins, &self.cfg.tiling, self.scale);
        for l in self.tiler.log.drain(..) {
            fx_log(|| format!("tuiles : {l}"));
        }
        if n > 0 {
            fx_log(|| format!("tuiles : {n} fenêtre(s) placée(s)"));
        }
    }

    /// Fenêtre lâchée après un déplacement : sur la tuile d'une autre fenêtre du même
    /// écran, elles échangent leur place ; sinon elle reprend la sienne.
    fn tile_drop(&mut self, h: HWND) {
        let mut cursor = POINT { x: 0, y: 0 };
        // SAFETY: sortie locale.
        unsafe { GetCursorPos(&mut cursor) };
        let mon = crate::tiler::Tiler::monitor_of(h);
        let target = self
            .tiler
            .order_of(mon)
            .iter()
            .copied()
            .filter(|w| *w != h as isize)
            .find(|w| crate::fx_overlay::visible_rect(*w as HWND).is_some_and(|r| r.contains(cursor.x, cursor.y)));
        if let Some(t) = target {
            self.tiler.swap(h, t as HWND);
        }
        self.windows = list_windows(self.hwnd);
        self.retile();
    }

    /// Enregistre la configuration modifiée par la barre elle-même.
    fn save_cfg(&mut self) {
        let r = self.cfg.save();
        fx_log(|| {
            format!(
                "configuration enregistrée : {r:?}, disposition {:?}",
                self.cfg.tiling.layout
            )
        });
        if r.is_ok() {
            self.cfg_stamp = std::fs::metadata(BarConfig::path()).and_then(|m| m.modified()).ok();
        }
    }

    /// Raccourci clavier des tuiles.
    fn hotkey(&mut self, id: i32) {
        fx_log(|| format!("raccourci {id}"));
        // SAFETY: lecture d'état.
        let fg = unsafe { GetForegroundWindow() };
        let t = &mut self.cfg.tiling;
        match id {
            1 => {
                t.enabled = !t.enabled;
                if !t.enabled {
                    self.tiler.restore_all();
                }
            }
            2 => t.layout = t.layout.next(),
            3 => self.tiler.promote(fg),
            4 | 5 => {
                if let Some(w) = self.tiler.neighbour(fg, if id == 4 { -1 } else { 1 }) {
                    activate(w);
                }
                return;
            }
            6 => t.master_percent = (t.master_percent + 5).min(prism_core::tiling::PERCENT_MAX),
            7 => t.master_percent = t.master_percent.saturating_sub(5).max(prism_core::tiling::PERCENT_MIN),
            8 => {
                let k = fg as isize;
                if !self.tiler.floating.remove(&k) {
                    self.tiler.floating.insert(k);
                    self.tiler.release(fg, true);
                }
            }
            _ => return,
        }
        if matches!(id, 1 | 2 | 6 | 7) {
            self.save_cfg();
        }
        self.windows = list_windows(self.hwnd);
        self.retile();
    }

    /// Démonstration mesurée : réduction, restauration, fermeture et ouverture
    /// simulées sur la fenêtre au premier plan (rien n'est réellement fermé).
    fn fx_demo(&mut self) {
        let saved = self.cfg.fx.clone();
        if !self.cfg.fx.enabled {
            self.cfg.fx = prism_core::fx::FxConfig {
                enabled: true,
                ..Default::default()
            };
        }
        // SAFETY: lecture de la fenêtre au premier plan.
        let fg = unsafe { GetForegroundWindow() };
        // Pas le terminal d'où l'on vient de lancer la démo.
        let console = |w: HWND| {
            let mut buf = [0u16; 64];
            // SAFETY: tampon local.
            let n = unsafe { GetClassNameW(w, buf.as_mut_ptr(), buf.len() as i32) };
            let class = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
            class == "ConsoleWindowClass" || class == "CASCADIA_HOSTING_WINDOW_CLASS"
        };
        let h = if self.fx_allowed(fg, true) && !console(fg) {
            Some(fg)
        } else {
            self.windows
                .iter()
                .map(|w| w.hwnd)
                .find(|w| self.fx_allowed(*w, true) && !console(*w))
        };
        if let Some(h) = h {
            activate(h);
            std::thread::sleep(Duration::from_millis(300));
            self.fx_click(h, "démo : réduire");
            std::thread::sleep(Duration::from_millis(500));
            self.fx_click(h, "démo : restaurer");
            std::thread::sleep(Duration::from_millis(500));
            if let Some(snap) = crate::fx_overlay::capture(h) {
                let target = self.target_for(h);
                let fx = self.cfg.fx.clone();
                if let Some(ex) = crate::fx_overlay::hide_temp(h) {
                    self.play_effect(
                        fx.close,
                        FxKind::Disappear,
                        &snap,
                        target,
                        Instant::now(),
                        "démo : fermer",
                        || {},
                    );
                    std::thread::sleep(Duration::from_millis(300));
                    self.play_effect(
                        fx.open,
                        FxKind::Appear,
                        &snap,
                        target,
                        Instant::now(),
                        "démo : ouvrir",
                        || {},
                    );
                    crate::fx_overlay::unhide(h, ex);
                }
            }
        }
        self.cfg.fx = saved;
    }

    fn paint(&self, hwnd: HWND, hdc: HDC) {
        let Some(panel) = self.panel(hwnd) else {
            return;
        };
        let (w, h) = client_size(hwnd);
        let scaled = |v: u32| (v as f32 * panel.scale).round() as i32;
        // SAFETY: double tampon GDI local, objets créés puis détruits ici.
        unsafe {
            let mem = CreateCompatibleDC(hdc);
            let bmp = CreateCompatibleBitmap(hdc, w, h);
            let old = SelectObject(mem, bmp as HGDIOBJ);
            fill(
                mem,
                RECT {
                    left: 0,
                    top: 0,
                    right: w,
                    bottom: h,
                },
                colors().bg,
            );
            SetBkMode(mem, TRANSPARENT as i32);
            let horizontal = self.cfg.edge.horizontal();
            let font = make_font(scaled(13), 400);
            let bold = make_font(scaled(15), 600);
            let small = make_font(scaled(11), 400);
            let fg = GetForegroundWindow();
            for p in &panel.items {
                let r = inset(p.rect, 3);
                match p.widget {
                    Widget::Start => {
                        text(mem, bold, colors().accent, r, "◆", DT_CENTER);
                    }
                    Widget::Windows => {
                        if let Some(win) = p
                            .index
                            .and_then(|i| panel.wins.get(i))
                            .and_then(|i| self.windows.get(*i))
                        {
                            fill(
                                mem,
                                r,
                                if win.hwnd == fg {
                                    colors().item_active
                                } else {
                                    colors().item
                                },
                            );
                            if win.hwnd == fg {
                                let line = if horizontal {
                                    RECT {
                                        left: r.left,
                                        top: r.bottom - 2,
                                        right: r.right,
                                        bottom: r.bottom,
                                    }
                                } else {
                                    RECT {
                                        left: r.left,
                                        top: r.top,
                                        right: r.left + 2,
                                        bottom: r.bottom,
                                    }
                                };
                                fill(mem, line, colors().accent);
                            }
                            let label = if horizontal {
                                win.title.clone()
                            } else {
                                win.title
                                    .chars()
                                    .next()
                                    .map(|c| c.to_uppercase().to_string())
                                    .unwrap_or_default()
                            };
                            let tr = RECT {
                                left: r.left + 8,
                                top: r.top,
                                right: r.right - 6,
                                bottom: r.bottom,
                            };
                            text(
                                mem,
                                font,
                                colors().text,
                                tr,
                                &label,
                                if horizontal { DT_LEFT } else { DT_CENTER },
                            );
                        }
                    }
                    Widget::Cpu => meter(mem, r, "CPU", self.sample.cpu, &self.cpu, horizontal, font, small),
                    Widget::Ram => meter(mem, r, "RAM", self.sample.ram, &self.ram, horizontal, font, small),
                    Widget::Gpu => match self.sample.gpu {
                        Some(g) => meter(mem, r, "GPU", g, &self.gpu, horizontal, font, small),
                        None => text(mem, small, colors().muted, r, "GPU —", DT_CENTER),
                    },
                    Widget::Network => {
                        let s = if horizontal {
                            format!(
                                "↓ {}  ↑ {}",
                                human_rate(self.sample.net_down),
                                human_rate(self.sample.net_up)
                            )
                        } else {
                            "NET".to_string()
                        };
                        text(mem, small, colors().text, r, &s, DT_CENTER);
                    }
                    Widget::GameMode => {
                        let (s, c) = if self.game {
                            ("● Jeu", colors().accent)
                        } else {
                            ("○ Jeu", colors().muted)
                        };
                        text(mem, font, c, r, s, DT_CENTER);
                    }
                    Widget::Clock => {
                        let mut t = std::mem::zeroed();
                        GetLocalTime(&mut t);
                        if horizontal && r.bottom - r.top >= 34 {
                            let top = RECT {
                                bottom: r.top + (r.bottom - r.top) * 3 / 5,
                                ..r
                            };
                            let bottom = RECT {
                                top: r.top + (r.bottom - r.top) * 11 / 20,
                                ..r
                            };
                            text(
                                mem,
                                bold,
                                colors().text,
                                top,
                                &format!("{:02}:{:02}", t.wHour, t.wMinute),
                                DT_CENTER,
                            );
                            text(
                                mem,
                                small,
                                colors().muted,
                                bottom,
                                &format!("{:02}/{:02}/{}", t.wDay, t.wMonth, t.wYear),
                                DT_CENTER,
                            );
                        } else {
                            text(
                                mem,
                                bold,
                                colors().text,
                                r,
                                &format!("{:02}:{:02}", t.wHour, t.wMinute),
                                DT_CENTER,
                            );
                        }
                    }
                }
            }
            BitBlt(hdc, 0, 0, w, h, mem, 0, 0, SRCCOPY);
            SelectObject(mem, old);
            for f in [font, bold, small] {
                DeleteObject(f as HGDIOBJ);
            }
            DeleteObject(bmp as HGDIOBJ);
            DeleteDC(mem);
        }
    }
}

fn gib(b: u64) -> String {
    format!("{:.1} Go", b as f64 / 1024.0 / 1024.0 / 1024.0)
}

fn date_fr(dow: u16, day: u16, month: u16, year: u16) -> String {
    const JOURS: [&str; 7] = ["dimanche", "lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi"];
    const MOIS: [&str; 12] = [
        "janvier",
        "février",
        "mars",
        "avril",
        "mai",
        "juin",
        "juillet",
        "août",
        "septembre",
        "octobre",
        "novembre",
        "décembre",
    ];
    let j = JOURS.get(dow as usize).unwrap_or(&"");
    let m = MOIS.get((month as usize).wrapping_sub(1)).unwrap_or(&"");
    format!("{j} {day} {m} {year}")
}

/// Courbe d'historique dans un rectangle (valeurs 0..=100).
unsafe fn graph_line(hdc: HDC, g: RECT, vals: &[f32], color: COLORREF) {
    fill(hdc, g, colors().item);
    if vals.len() < 2 || g.right <= g.left {
        return;
    }
    let n = vals.len() as i32;
    let pts: Vec<POINT> = vals
        .iter()
        .enumerate()
        .map(|(i, v)| POINT {
            x: g.left + (g.right - g.left) * i as i32 / (n - 1),
            y: g.bottom - ((g.bottom - g.top) as f32 * v / 100.0) as i32,
        })
        .collect();
    let pen = CreatePen(PS_SOLID, 2, color);
    let old = SelectObject(hdc, pen as HGDIOBJ);
    Polyline(hdc, pts.as_ptr(), pts.len() as i32);
    SelectObject(hdc, old);
    DeleteObject(pen as HGDIOBJ);
}

fn inset(r: Rect, d: i32) -> RECT {
    RECT {
        left: r.left + d,
        top: r.top + d,
        right: r.right - d,
        bottom: r.bottom - d,
    }
}

unsafe fn fill(hdc: HDC, r: RECT, c: COLORREF) {
    let b = CreateSolidBrush(c);
    FillRect(hdc, &r, b);
    DeleteObject(b as HGDIOBJ);
}

unsafe fn make_font(px: i32, weight: i32) -> HFONT {
    let face = wide("Segoe UI");
    CreateFontW(
        -px,
        0,
        0,
        0,
        weight,
        0,
        0,
        0,
        DEFAULT_CHARSET as u32,
        0,
        0,
        CLEARTYPE_QUALITY as u32,
        0,
        face.as_ptr(),
    )
}

unsafe fn text(hdc: HDC, font: HFONT, color: COLORREF, r: RECT, s: &str, align: DRAW_TEXT_FORMAT) {
    let old = SelectObject(hdc, font as HGDIOBJ);
    SetTextColor(hdc, color);
    let mut w: Vec<u16> = s.encode_utf16().collect();
    let mut rr = r;
    DrawTextW(
        hdc,
        w.as_mut_ptr(),
        w.len() as i32,
        &mut rr,
        align | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
    );
    SelectObject(hdc, old);
}

/// Jauge : libellé, pourcentage et mini-graphe de l'historique.
#[allow(clippy::too_many_arguments)]
unsafe fn meter(
    hdc: HDC,
    r: RECT,
    label: &str,
    value: f32,
    hist: &History,
    horizontal: bool,
    font: HFONT,
    small: HFONT,
) {
    let color = if value > 85.0 {
        colors().warn
    } else if value > 60.0 {
        colors().accent
    } else {
        colors().ok
    };
    if !horizontal {
        text(
            hdc,
            small,
            colors().muted,
            RECT {
                bottom: r.top + (r.bottom - r.top) / 2,
                ..r
            },
            label,
            DT_CENTER,
        );
        text(
            hdc,
            font,
            color,
            RECT {
                top: r.top + (r.bottom - r.top) / 2,
                ..r
            },
            &format!("{value:.0}"),
            DT_CENTER,
        );
        return;
    }
    let split = r.left + (r.right - r.left) * 9 / 20;
    text(
        hdc,
        small,
        colors().muted,
        RECT {
            right: split,
            bottom: r.top + (r.bottom - r.top) / 2 + 2,
            ..r
        },
        label,
        DT_LEFT,
    );
    text(
        hdc,
        font,
        color,
        RECT {
            right: split,
            top: r.top + (r.bottom - r.top) / 2 - 2,
            ..r
        },
        &format!("{value:.0} %"),
        DT_LEFT,
    );
    let g = RECT {
        left: split + 4,
        top: r.top + 6,
        right: r.right - 2,
        bottom: r.bottom - 6,
    };
    let vals = hist.values();
    if vals.len() >= 2 && g.right > g.left {
        let n = vals.len() as i32;
        let pts: Vec<POINT> = vals
            .iter()
            .enumerate()
            .map(|(i, v)| POINT {
                x: g.left + (g.right - g.left) * i as i32 / (n - 1),
                y: g.bottom - ((g.bottom - g.top) as f32 * v / 100.0) as i32,
            })
            .collect();
        let pen = CreatePen(PS_SOLID, 2, color);
        let old = SelectObject(hdc, pen as HGDIOBJ);
        Polyline(hdc, pts.as_ptr(), pts.len() as i32);
        SelectObject(hdc, old);
        DeleteObject(pen as HGDIOBJ);
    }
}

/// Fenêtres de premier niveau qu'une barre des tâches montrerait.
fn list_windows(own: HWND) -> Vec<TaskWin> {
    unsafe extern "system" fn cb(hwnd: HWND, lp: LPARAM) -> windows_sys::core::BOOL {
        let out = &mut *(lp as *mut Vec<TaskWin>);
        if IsWindowVisible(hwnd) == 0 || !GetWindow(hwnd, GW_OWNER).is_null() {
            return 1;
        }
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        if ex & WS_EX_TOOLWINDOW != 0 {
            return 1;
        }
        let mut cloaked: u32 = 0;
        DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED as u32, &mut cloaked as *mut _ as *mut c_void, 4);
        if cloaked != 0 {
            return 1;
        }
        let mut buf = [0u16; 256];
        let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        if n > 0 {
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            out.push(TaskWin {
                hwnd,
                title: String::from_utf16_lossy(&buf[..n as usize]),
                pid,
            });
        }
        1
    }
    let mut out: Vec<TaskWin> = Vec::new();
    // SAFETY: le pointeur vers `out` reste valide pendant l'énumération synchrone.
    unsafe { EnumWindows(Some(cb), &mut out as *mut _ as LPARAM) };
    out.retain(|w| w.hwnd != own && w.title != "Program Manager");
    out
}

/// La fenêtre couvre-t-elle tout son écran (jeu ou vidéo plein écran) ?
fn is_fullscreen(hwnd: HWND) -> bool {
    // SAFETY: sorties locales ; MonitorFromWindow renvoie toujours un écran.
    unsafe {
        let mut r = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        GetWindowRect(hwnd, &mut r);
        let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(mon, &mut mi) == 0 {
            return false;
        }
        let m = mi.rcMonitor;
        r.left <= m.left && r.top <= m.top && r.right >= m.right && r.bottom >= m.bottom
    }
}

fn activate(hwnd: HWND) {
    // SAFETY: fenêtre d'un autre processus ; le clic sur la barre nous donne le droit
    // de passer une fenêtre au premier plan.
    unsafe {
        if IsIconic(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
        }
        SetForegroundWindow(hwnd);
    }
}

fn press_win_key() {
    let key = |flags| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VK_LWIN,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let inputs = [key(0), key(KEYEVENTF_KEYUP)];
    // SAFETY: tableau local de deux entrées clavier.
    unsafe { SendInput(2, inputs.as_ptr(), size_of::<INPUT>() as i32) };
}

fn open_prism_ui() {
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
    {
        let _ = std::process::Command::new(dir.join("prism-ui.exe")).spawn();
    }
}

/// Un menu, une liste déroulante, une infobulle ou une boîte de dialogue vient
/// d'apparaître (événement d'accessibilité officiel, reçu hors du processus de
/// l'appli : aucune injection). On lui applique l'opacité de sa catégorie.
unsafe extern "system" fn on_object_show(
    _hook: windows_sys::Win32::UI::Accessibility::HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _thread: u32,
    _time: u32,
) {
    if hwnd.is_null() || id_object != OBJID_WINDOW || id_child != 0 {
        return;
    }
    // Effets : l'événement est mis en file et traité par la barre juste après.
    if GetAncestor(hwnd, GA_ROOT) == hwnd {
        FX_QUEUE.with(|q| q.borrow_mut().push((event, hwnd as isize, Instant::now())));
        let bar = BAR_HWND.with(|b| b.get());
        if bar != 0 {
            PostMessageW(bar as HWND, WM_FX, 0, 0);
        }
    }
    if event != EVENT_OBJECT_SHOW_ID {
        return;
    }
    let mut buf = [0u16; 64];
    let n = GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
    if n <= 0 {
        return;
    }
    let class = String::from_utf16_lossy(&buf[..n as usize]);
    with_bar(|b| {
        let Some(op) = b.cfg.element_opacity.for_class(&class) else {
            return;
        };
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        let path = crate::win::process_path(pid);
        let exe = path
            .as_deref()
            .and_then(|p| p.rsplit('\\').next())
            .unwrap_or_default()
            .to_string();
        if prism_core::classify::is_game_process(&exe, path.as_deref(), &b.game_cfg) {
            return; // jamais dans un jeu
        }
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        if ex as u32 & WS_EX_LAYERED != 0 {
            return; // l'élément gère déjà sa transparence
        }
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | WS_EX_LAYERED as isize);
        SetLayeredWindowAttributes(hwnd, 0, (op as u32 * 255 / 100) as u8, LWA_ALPHA);
    });
}

unsafe extern "system" fn desk_wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            with_bar(|b| b.paint_desk(hwnd, hdc));
            EndPaint(hwnd, &ps);
            0
        }
        WM_ERASEBKGND => 1,
        // Attraper le widget n'importe où le déplace.
        WM_NCHITTEST => HTCAPTION as LRESULT,
        // Toujours au fond, derrière les fenêtres.
        WM_WINDOWPOSCHANGING => {
            let pos = &mut *(lp as *mut WINDOWPOS);
            pos.hwndInsertAfter = HWND_BOTTOM;
            0
        }
        WM_EXITSIZEMOVE => {
            with_bar(|b| b.desk_moved(hwnd));
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_FX => {
            with_bar(|b| b.process_fx());
            0
        }
        WM_FX_DEMO => {
            with_bar(|b| b.fx_demo());
            0
        }
        WM_FX_DRAG => {
            with_bar(|b| b.drag_frame());
            0
        }
        WM_HOTKEY => {
            with_bar(|b| b.hotkey(wp as i32));
            0
        }
        WM_TIMER => {
            with_bar(|b| b.tick());
            0
        }
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            with_bar(|b| b.paint(hwnd, hdc));
            EndPaint(hwnd, &ps);
            0
        }
        WM_ERASEBKGND => 1,
        WM_LBUTTONDOWN => {
            let x = (lp & 0xffff) as i16 as i32;
            let y = ((lp >> 16) & 0xffff) as i16 as i32;
            with_bar(|b| b.prepare_click(hwnd, x, y));
            0
        }
        WM_LBUTTONUP => {
            let x = (lp & 0xffff) as i16 as i32;
            let y = ((lp >> 16) & 0xffff) as i16 as i32;
            with_bar(|b| b.click(hwnd, x, y));
            0
        }
        WM_APPBAR => {
            match wp as u32 {
                ABN_POSCHANGED => {
                    with_bar(|b| b.dock());
                }
                ABN_FULLSCREENAPP => {
                    // Un jeu ou une vidéo passe en plein écran : la barre de son écran s'efface.
                    with_bar(|b| b.fullscreen_changed(hwnd, lp != 0));
                }
                _ => {}
            }
            0
        }
        WM_DPICHANGED => {
            with_bar(|b| b.dock());
            0
        }
        WM_DISPLAYCHANGE | WM_SETTINGCHANGE => {
            with_bar(|b| b.dock());
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_CLOSE | WM_ENDSESSION => {
            // Quelle que soit la barre visée, c'est toute la Prism Bar qui s'arrête.
            let main = BAR_HWND.with(|b| b.get());
            DestroyWindow(if main != 0 { main as HWND } else { hwnd });
            0
        }
        // Barre d'un écran débranché : rien d'autre à faire.
        WM_DESTROY if BAR_HWND.with(|b| b.get()) != hwnd as isize => 0,
        WM_DESTROY => {
            with_bar(|b| {
                b.drag_end();
                b.tiler.restore_all();
                b.restore_opacity();
                for d in b.desk.drain(..) {
                    DestroyWindow(d.hwnd);
                }
                for p in b.panels.drain(..) {
                    let mut d = appbar_data(p.hwnd);
                    SHAppBarMessage(ABM_REMOVE, &mut d);
                    if p.hwnd != b.hwnd {
                        DestroyWindow(p.hwnd);
                    }
                }
                if b.cfg.hide_windows_taskbar {
                    show_taskbar();
                }
            });
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

/// Lance la barre (boucle de messages jusqu'à fermeture). Une seule instance.
pub fn run() -> Result<(), String> {
    // SAFETY: appels Win32 d'initialisation, puis boucle de messages classique.
    unsafe {
        let mutex_name = wide("Local\\PrismBar");
        let m = CreateMutexW(null(), 1, mutex_name.as_ptr());
        if m.is_null()
            || windows_sys::Win32::Foundation::GetLastError() == windows_sys::Win32::Foundation::ERROR_ALREADY_EXISTS
        {
            return Err("Prism Bar tourne déjà".into());
        }
        // Une barre précédente arrêtée de force pendant un effet : fenêtres rendues visibles.
        crate::fx_overlay::recover_hidden();
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let hinst = GetModuleHandleW(null());
        let class = wide(CLASS);
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinst,
            hIcon: null_mut(),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hbrBackground: null_mut(),
            lpszMenuName: null(),
            lpszClassName: class.as_ptr(),
            hIconSm: null_mut(),
        };
        if RegisterClassExW(&wc) == 0 {
            return Err("RegisterClassExW a échoué".into());
        }
        let desk_class = wide(DESK_CLASS);
        let wc_desk = WNDCLASSEXW {
            lpfnWndProc: Some(desk_wndproc),
            lpszClassName: desk_class.as_ptr(),
            hCursor: LoadCursorW(null_mut(), IDC_SIZEALL),
            ..wc
        };
        if RegisterClassExW(&wc_desk) == 0 {
            return Err("RegisterClassExW (widgets) a échoué".into());
        }
        let hwnd = create_panel_window();
        if hwnd.is_null() {
            return Err("CreateWindowExW a échoué".into());
        }
        BAR_HWND.with(|b| b.set(hwnd as isize));
        let cfg = BarConfig::load();
        set_colors(&cfg.theme);
        if cfg.hide_windows_taskbar {
            hide_taskbar();
        }
        let scale = GetDpiForWindow(hwnd).max(96) as f32 / 96.0;
        BAR.with(|b| {
            *b.borrow_mut() = Some(Bar {
                hwnd,
                cfg_stamp: std::fs::metadata(BarConfig::path()).and_then(|m| m.modified()).ok(),
                cfg,
                metrics: Metrics::new(),
                sample: Sample::default(),
                cpu: History::new(30),
                ram: History::new(30),
                gpu: History::new(30),
                windows: Vec::new(),
                panels: vec![Panel {
                    hwnd,
                    monitor: 0,
                    rect: Rect::default(),
                    scale,
                    items: Vec::new(),
                    wins: Vec::new(),
                    hidden: false,
                }],
                game: false,
                desk_hidden: false,
                scale,
                desk: Vec::new(),
                game_cfg: {
                    let mut c =
                        prism_core::paths::load_config().unwrap_or_else(|_| prism_core::config::Config::builtin());
                    c.lists
                        .game_roots
                        .extend(prism_core::library::game_roots(&crate::installed_games()));
                    c
                },
                translucent: std::collections::HashMap::new(),
                overlay: crate::fx_overlay::Overlay::new(),
                snaps: HashMap::new(),
                skip: HashMap::new(),
                stats: prism_core::fx::load_stats(),
                drag: None,
                geo: HashMap::new(),
                loc_hook: None,
                tiler: crate::tiler::Tiler::with_saved_originals(),
            })
        });
        with_bar(|b| {
            // Barre précédente arrêtée de force avec les tuiles actives, désactivées
            // depuis : les fenêtres retrouvent leur place.
            if !b.cfg.tiling.enabled {
                b.tiler.restore_all();
            }
            b.dock();
            b.tick();
        });
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        SetTimer(hwnd, TIMER_ID, 1000, None);
        {
            use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
                RegisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_WIN,
            };
            for (id, vk, label) in HOTKEYS {
                if RegisterHotKey(hwnd, id, MOD_WIN | MOD_CONTROL | MOD_ALT | MOD_NOREPEAT, vk) == 0 {
                    fx_log(|| format!("raccourci déjà pris par une autre appli : {label}"));
                }
            }
        }
        use windows_sys::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent};
        let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
        // Apparitions/disparitions (transparence par élément, ouverture/fermeture),
        // premier plan, réduction/restauration (effets).
        let hook = SetWinEventHook(
            EVENT_OBJECT_SHOW,
            EVENT_OBJECT_HIDE,
            null_mut(),
            Some(on_object_show),
            0,
            0,
            flags,
        );
        let hook_fg = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            null_mut(),
            Some(on_object_show),
            0,
            0,
            flags,
        );
        let hook_min = SetWinEventHook(
            EVENT_SYSTEM_MINIMIZESTART,
            EVENT_SYSTEM_MINIMIZEEND,
            null_mut(),
            Some(on_object_show),
            0,
            0,
            flags,
        );
        let hook_move = SetWinEventHook(
            EVENT_SYSTEM_MOVESIZESTART,
            EVENT_SYSTEM_MOVESIZEEND,
            null_mut(),
            Some(on_object_show),
            0,
            0,
            flags,
        );
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        if let Some((h, _)) = with_bar(|b| b.loc_hook.take()).flatten() {
            UnhookWinEvent(h as _);
        }
        for h in [hook, hook_fg, hook_min, hook_move] {
            if !h.is_null() {
                UnhookWinEvent(h);
            }
        }
    }
    Ok(())
}

/// Ferme une Prism Bar en cours (depuis l'interface ou la ligne de commande).
pub fn stop() -> bool {
    let class = wide(CLASS);
    // SAFETY: recherche par nom de classe puis message de fermeture.
    unsafe {
        let h = FindWindowW(class.as_ptr(), null());
        if h.is_null() {
            return false;
        }
        PostMessageW(h, WM_CLOSE, 0, 0);
        true
    }
}

/// Demande à la barre une démonstration mesurée des effets. `false` si elle ne tourne pas.
pub fn fx_demo() -> bool {
    let class = wide(CLASS);
    // SAFETY: recherche par nom de classe puis message à notre propre barre.
    unsafe {
        let h = FindWindowW(class.as_ptr(), null());
        if h.is_null() {
            return false;
        }
        PostMessageW(h, WM_FX_DEMO, 0, 0);
        true
    }
}

/// Une Prism Bar tourne-t-elle ?
pub fn running() -> bool {
    let class = wide(CLASS);
    // SAFETY: recherche par nom de classe.
    unsafe { !FindWindowW(class.as_ptr(), null()).is_null() }
}
