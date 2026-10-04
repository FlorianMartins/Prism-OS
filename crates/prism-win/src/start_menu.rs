//! Fenêtre du menu Démarrer de Prism (logique : `prism_core::start_menu`).
//!
//! Ouverte par le bouton Démarrer de la Prism Bar ou Alt+F1, fermée par Échap, un clic
//! ailleurs ou le lancement d'une appli. Dessinée en GDI dans les couleurs du thème de
//! la barre, coins arrondis et ombre de Windows 11. Aucun hook clavier : la touche
//! Windows garde le menu de Windows.

use std::cell::RefCell;
use std::ptr::{null, null_mut};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use prism_core::bar::{BarConfig, Edge};
use prism_core::start_menu::{self, Action, App, Key, Menu, Power};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreatePen, CreateSolidBrush, DeleteDC,
    DeleteObject, EndPaint, GetMonitorInfoW, InvalidateRect, MonitorFromRect, RoundRect, SelectObject, SetBkMode,
    DT_CENTER, DT_LEFT, HDC, HGDIOBJ, MONITORINFO, MONITOR_DEFAULTTONEAREST, PAINTSTRUCT, PS_SOLID, SRCCOPY,
    TRANSPARENT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{VK_BACK, VK_DOWN, VK_ESCAPE, VK_RETURN, VK_UP};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, LoadCursorW, PostMessageW, RegisterClassExW,
    SetForegroundWindow, ShowWindow, CS_DROPSHADOW, IDC_ARROW, SW_SHOW, SW_SHOWNORMAL, WM_ACTIVATE, WM_APP, WM_CHAR,
    WM_CLOSE, WM_DESTROY, WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT, WM_RBUTTONUP,
    WNDCLASSEXW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

use crate::bar_app::{colors, fill, make_font, text, wide};

const CLASS: &str = "PrismStartMenu";
/// Liste des applis mise à jour : redessiner.
const WM_APPS: u32 = WM_APP + 41;
/// Liste des applis relue au plus toutes les 10 minutes.
const REFRESH: Duration = Duration::from_secs(600);

static APPS: Mutex<Option<(Instant, Vec<App>)>> = Mutex::new(None);
static LOADING: Mutex<bool> = Mutex::new(false);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Hit {
    Row(usize),
    Power(Power),
    Prism,
}

struct State {
    hwnd: HWND,
    menu: Menu,
    apps: Vec<App>,
    pinned: Vec<String>,
    hover: Option<Hit>,
    scale: f32,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
    /// Fermé à l'instant (clic sur le bouton Démarrer pendant qu'il était ouvert : le
    /// clic ferme, il ne doit pas rouvrir).
    static CLOSED_AT: std::cell::Cell<Option<Instant>> = const { std::cell::Cell::new(None) };
}

fn cache_path() -> std::path::PathBuf {
    prism_core::paths::user_dir().join("start-apps.json")
}

/// Applis connues : cache disque au premier appel, relues en arrière-plan si besoin.
fn apps(notify: Option<HWND>) -> Vec<App> {
    let mut guard = APPS.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        if let Ok(b) = std::fs::read(cache_path()) {
            if let Some(list) = start_menu::from_cache(&b) {
                // Cache ancien : relu tout de suite en arrière-plan.
                *guard = Some((Instant::now() - REFRESH, list));
            }
        }
    }
    let stale = guard.as_ref().map_or(true, |(t, _)| t.elapsed() >= REFRESH);
    let list = guard.as_ref().map(|(_, l)| l.clone()).unwrap_or_default();
    drop(guard);
    if stale {
        refresh(notify);
    }
    list
}

fn refresh(notify: Option<HWND>) {
    {
        let mut l = LOADING.lock().unwrap_or_else(|e| e.into_inner());
        if *l {
            return;
        }
        *l = true;
    }
    let notify = notify.map(|h| h as isize);
    std::thread::spawn(move || {
        use std::os::windows::process::CommandExt;
        let out = std::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "[Console]::OutputEncoding=[Text.Encoding]::UTF8; Get-StartApps | Select-Object Name,AppID | ConvertTo-Json -Compress",
            ])
            .creation_flags(0x0800_0000)
            .stdin(std::process::Stdio::null())
            .output();
        if let Ok(o) = out {
            let list = start_menu::parse_start_apps(&String::from_utf8_lossy(&o.stdout));
            if !list.is_empty() {
                let _ = std::fs::create_dir_all(prism_core::paths::user_dir());
                let _ = std::fs::write(cache_path(), start_menu::to_cache(&list));
                *APPS.lock().unwrap_or_else(|e| e.into_inner()) = Some((Instant::now(), list));
                if let Some(h) = notify {
                    // SAFETY: message à notre fenêtre ; sans effet si elle est fermée.
                    unsafe { PostMessageW(h as HWND, WM_APPS, 0, 0) };
                }
            }
        }
        *LOADING.lock().unwrap_or_else(|e| e.into_inner()) = false;
    });
}

/// Précharge la liste (au démarrage de la barre) : le premier clic est instantané.
pub fn preload() {
    let _ = apps(None);
}

pub fn is_open() -> bool {
    STATE.with(|s| s.borrow().is_some())
}

/// Ouvre (ou ferme) le menu, ancré au rectangle `anchor` (écran) du bouton Démarrer.
pub fn toggle(anchor: RECT, edge: Edge, scale: f32) {
    if is_open() {
        close();
        return;
    }
    if CLOSED_AT
        .with(|c| c.get())
        .is_some_and(|t| t.elapsed() < Duration::from_millis(250))
    {
        return;
    }
    open(anchor, edge, scale);
}

fn open(anchor: RECT, edge: Edge, scale: f32) {
    register();
    let s = |v: f32| (v * scale).round() as i32;
    let (w, h) = (s(460.0), s(600.0));
    let gap = s(8.0);
    let (mut x, mut y) = match edge {
        Edge::Bottom => (anchor.left, anchor.top - h - gap),
        Edge::Top => (anchor.left, anchor.bottom + gap),
        Edge::Left => (anchor.right + gap, anchor.top),
        Edge::Right => (anchor.left - w - gap, anchor.top),
    };
    // SAFETY: structures locales ; écran toujours trouvé (le plus proche).
    unsafe {
        let mon = MonitorFromRect(&anchor, MONITOR_DEFAULTTONEAREST);
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(mon, &mut mi) != 0 {
            let r = mi.rcWork;
            x = x.clamp(r.left + gap, (r.right - w - gap).max(r.left));
            y = y.clamp(r.top + gap, (r.bottom - h - gap).max(r.top));
        }
        let class = wide(CLASS);
        let title = wide("Prism");
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            class.as_ptr(),
            title.as_ptr(),
            WS_POPUP,
            x,
            y,
            w,
            h,
            null_mut(),
            null_mut(),
            GetModuleHandleW(null()),
            null(),
        );
        if hwnd.is_null() {
            return;
        }
        // Coins arrondis de Windows 11 (sans effet sur Windows 10).
        let round: u32 = 2;
        DwmSetWindowAttribute(hwnd, 33, &round as *const u32 as *const _, 4);
        let pinned = BarConfig::load().start_pinned;
        let list = apps(Some(hwnd));
        STATE.with(|st| {
            *st.borrow_mut() = Some(State {
                hwnd,
                menu: Menu::open(&list, &pinned),
                apps: list,
                pinned,
                hover: None,
                scale,
            })
        });
        ShowWindow(hwnd, SW_SHOW);
        SetForegroundWindow(hwnd);
    }
}

pub fn close() {
    let hwnd = STATE.with(|s| s.borrow_mut().take().map(|st| st.hwnd));
    if let Some(h) = hwnd {
        CLOSED_AT.with(|c| c.set(Some(Instant::now())));
        // SAFETY: notre fenêtre.
        unsafe { DestroyWindow(h) };
    }
}

fn register() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let class = wide(CLASS);
        // SAFETY: classe enregistrée une fois ; chaîne vivante pendant l'appel.
        unsafe {
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: CS_DROPSHADOW,
                lpfnWndProc: Some(wndproc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: GetModuleHandleW(null()),
                hIcon: null_mut(),
                hCursor: LoadCursorW(null_mut(), IDC_ARROW),
                hbrBackground: null_mut(),
                lpszMenuName: null(),
                lpszClassName: class.as_ptr(),
                hIconSm: null_mut(),
            };
            RegisterClassExW(&wc);
        }
    });
}

/// Zones de la fenêtre.
struct Layout {
    search: RECT,
    list: RECT,
    row_h: i32,
    visible: usize,
    buttons: Vec<(Hit, RECT)>,
}

fn layout(w: i32, h: i32, scale: f32) -> Layout {
    let s = |v: f32| (v * scale).round() as i32;
    let pad = s(14.0);
    let search = RECT {
        left: pad,
        top: pad,
        right: w - pad,
        bottom: pad + s(42.0),
    };
    let row_h = s(38.0);
    let power_h = s(44.0);
    let list = RECT {
        left: pad,
        top: search.bottom + s(12.0),
        right: w - pad,
        bottom: h - pad - power_h - s(10.0),
    };
    let visible = ((list.bottom - list.top) / row_h.max(1)).max(1) as usize;
    let hits: Vec<Hit> = std::iter::once(Hit::Prism)
        .chain(Power::ALL.iter().map(|p| Hit::Power(*p)))
        .collect();
    let bw = (w - 2 * pad - (hits.len() as i32 - 1) * s(6.0)) / hits.len() as i32;
    let buttons = hits
        .into_iter()
        .enumerate()
        .map(|(i, hit)| {
            let left = pad + i as i32 * (bw + s(6.0));
            (
                hit,
                RECT {
                    left,
                    top: h - pad - power_h,
                    right: left + bw,
                    bottom: h - pad,
                },
            )
        })
        .collect();
    Layout {
        search,
        list,
        row_h,
        visible,
        buttons,
    }
}

fn client(hwnd: HWND) -> (i32, i32) {
    let mut r: RECT = unsafe { std::mem::zeroed() };
    // SAFETY: notre fenêtre, sortie locale.
    unsafe { GetClientRect(hwnd, &mut r) };
    (r.right - r.left, r.bottom - r.top)
}

fn hit_at(st: &State, x: i32, y: i32) -> Option<Hit> {
    let (w, h) = client(st.hwnd);
    let l = layout(w, h, st.scale);
    let inside = |r: &RECT| x >= r.left && x < r.right && y >= r.top && y < r.bottom;
    if inside(&l.list) {
        let rank = st.menu.scroll + ((y - l.list.top) / l.row_h) as usize;
        return (rank < st.menu.shown.len()).then_some(Hit::Row(rank));
    }
    l.buttons.iter().find(|(_, r)| inside(r)).map(|(h, _)| *h)
}

unsafe fn round_fill(hdc: HDC, r: RECT, radius: i32, c: u32) {
    let b = CreateSolidBrush(c);
    let p = CreatePen(PS_SOLID, 1, c);
    let ob = SelectObject(hdc, b as HGDIOBJ);
    let op = SelectObject(hdc, p as HGDIOBJ);
    RoundRect(hdc, r.left, r.top, r.right, r.bottom, radius, radius);
    SelectObject(hdc, ob);
    SelectObject(hdc, op);
    DeleteObject(b as HGDIOBJ);
    DeleteObject(p as HGDIOBJ);
}

unsafe fn paint(st: &State, hdc: HDC, w: i32, h: i32) {
    let c = colors();
    let s = |v: f32| (v * st.scale).round() as i32;
    let l = layout(w, h, st.scale);
    fill(
        hdc,
        RECT {
            left: 0,
            top: 0,
            right: w,
            bottom: h,
        },
        c.bg,
    );
    SetBkMode(hdc, TRANSPARENT as i32);
    let font = make_font(s(15.0), 400);
    let bold = make_font(s(15.0), 600);
    let small = make_font(s(12.0), 400);
    // Recherche.
    round_fill(hdc, l.search, s(10.0), c.item);
    let mut sr = l.search;
    sr.left += s(14.0);
    if st.menu.query.is_empty() {
        text(hdc, font, c.muted, sr, "Rechercher une appli…", DT_LEFT);
    } else {
        text(hdc, font, c.text, sr, &format!("{}|", st.menu.query), DT_LEFT);
    }
    // Liste.
    if st.menu.shown.is_empty() {
        let msg = if st.apps.is_empty() {
            "Chargement des applis…"
        } else {
            "Aucune appli trouvée"
        };
        let mut r = l.list;
        r.bottom = r.top + l.row_h;
        text(hdc, font, c.muted, r, msg, DT_CENTER);
    }
    for (row, rank) in (st.menu.scroll..st.menu.shown.len()).take(l.visible).enumerate() {
        let app = &st.apps[st.menu.shown[rank]];
        let top = l.list.top + row as i32 * l.row_h;
        let r = RECT {
            left: l.list.left,
            top,
            right: l.list.right,
            bottom: top + l.row_h - s(2.0),
        };
        let selected = rank == st.menu.sel;
        if selected || st.hover == Some(Hit::Row(rank)) {
            round_fill(hdc, r, s(8.0), if selected { c.item_active } else { c.item });
        }
        // Pastille avec l'initiale.
        let b = s(26.0);
        let badge = RECT {
            left: r.left + s(8.0),
            top: r.top + (r.bottom - r.top - b) / 2,
            right: r.left + s(8.0) + b,
            bottom: r.top + (r.bottom - r.top - b) / 2 + b,
        };
        round_fill(hdc, badge, s(7.0), c.item_active);
        let initial: String = app
            .name
            .chars()
            .next()
            .map(|ch| ch.to_uppercase().collect())
            .unwrap_or_default();
        text(hdc, bold, c.accent, badge, &initial, DT_CENTER);
        let mut nr = r;
        nr.left = badge.right + s(12.0);
        nr.right -= s(30.0);
        text(hdc, font, c.text, nr, &app.name, DT_LEFT);
        if st.pinned.iter().any(|p| p.eq_ignore_ascii_case(&app.id)) {
            let mut pr = r;
            pr.left = r.right - s(26.0);
            text(hdc, small, c.accent, pr, "★", DT_CENTER);
        }
    }
    // Boutons du bas.
    for (hit, r) in &l.buttons {
        let hover = st.hover == Some(*hit);
        round_fill(hdc, *r, s(10.0), if hover { c.item_active } else { c.item });
        let (label, color) = match hit {
            Hit::Prism => ("Prism", c.accent),
            Hit::Power(p) => (p.label(), c.text),
            Hit::Row(_) => ("", c.text),
        };
        text(hdc, small, color, *r, label, DT_CENTER);
    }
    for f in [font, bold, small] {
        DeleteObject(f as HGDIOBJ);
    }
}

fn launch(id: &str) {
    let verb = wide("open");
    let exe = wide("explorer.exe");
    let arg = wide(&format!("shell:AppsFolder\\{id}"));
    // SAFETY: chaînes vivantes pendant l'appel.
    unsafe {
        ShellExecuteW(
            null_mut(),
            verb.as_ptr(),
            exe.as_ptr(),
            arg.as_ptr(),
            null(),
            SW_SHOWNORMAL,
        )
    };
}

fn power(p: Power) {
    use std::os::windows::process::CommandExt;
    let hidden = |args: &[&str]| {
        let _ = std::process::Command::new("shutdown.exe")
            .args(args)
            .creation_flags(0x0800_0000)
            .spawn();
    };
    match p {
        // SAFETY: appels sans paramètre pointeur.
        Power::Verrouiller => unsafe {
            windows_sys::Win32::System::Shutdown::LockWorkStation();
        },
        Power::Veille => unsafe {
            windows_sys::Win32::System::Power::SetSuspendState(false, false, false);
        },
        Power::Redemarrer => hidden(&["/r", "/t", "0"]),
        Power::Arreter => hidden(&["/s", "/t", "0"]),
    }
}

fn open_prism() {
    if let Ok(exe) = std::env::current_exe() {
        let ui = exe.with_file_name("prism-ui.exe");
        let _ = std::process::Command::new(ui).spawn();
    }
}

/// Action demandée hors de l'emprunt de l'état (lancer ferme la fenêtre).
enum After {
    Nothing,
    Redraw,
    Close,
    Launch(String),
    Power(Power),
    Prism,
    SavePins(Vec<String>),
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let after = match msg {
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            let (w, h) = client(hwnd);
            // Double tampon : pas de scintillement.
            let mem = CreateCompatibleDC(hdc);
            let bmp = CreateCompatibleBitmap(hdc, w, h);
            let old = SelectObject(mem, bmp as HGDIOBJ);
            STATE.with(|s| {
                if let Some(st) = s.borrow().as_ref() {
                    paint(st, mem, w, h);
                }
            });
            BitBlt(hdc, 0, 0, w, h, mem, 0, 0, SRCCOPY);
            SelectObject(mem, old);
            DeleteObject(bmp as HGDIOBJ);
            DeleteDC(mem);
            EndPaint(hwnd, &ps);
            return 0;
        }
        WM_ERASEBKGND => return 1,
        WM_APPS => STATE.with(|s| {
            let mut b = s.borrow_mut();
            let Some(st) = b.as_mut() else { return After::Nothing };
            st.apps = apps(None);
            st.menu.refresh(&st.apps, &st.pinned);
            After::Redraw
        }),
        WM_CHAR | WM_KEYDOWN => {
            let key = if msg == WM_CHAR {
                match char::from_u32(wp as u32) {
                    Some(ch) if !ch.is_control() => Some(Key::Char(ch)),
                    _ => None,
                }
            } else {
                match wp as u16 {
                    VK_UP => Some(Key::Up),
                    VK_DOWN => Some(Key::Down),
                    VK_RETURN => Some(Key::Enter),
                    VK_ESCAPE => Some(Key::Escape),
                    VK_BACK => Some(Key::Backspace),
                    _ => None,
                }
            };
            match key {
                None => After::Nothing,
                Some(k) => STATE.with(|s| {
                    let mut b = s.borrow_mut();
                    let Some(st) = b.as_mut() else { return After::Nothing };
                    let (w, h) = client(hwnd);
                    let visible = layout(w, h, st.scale).visible;
                    match st.menu.key(k, &st.apps, &st.pinned, visible) {
                        Action::Rien => After::Nothing,
                        Action::Redessiner => After::Redraw,
                        Action::Lancer(id) => After::Launch(id),
                        Action::Fermer => After::Close,
                    }
                }),
            }
        }
        WM_MOUSEMOVE => STATE.with(|s| {
            let mut b = s.borrow_mut();
            let Some(st) = b.as_mut() else { return After::Nothing };
            let h = hit_at(st, (lp & 0xffff) as i16 as i32, ((lp >> 16) & 0xffff) as i16 as i32);
            if h != st.hover {
                st.hover = h;
                After::Redraw
            } else {
                After::Nothing
            }
        }),
        WM_MOUSEWHEEL => STATE.with(|s| {
            let mut b = s.borrow_mut();
            let Some(st) = b.as_mut() else { return After::Nothing };
            let delta = ((wp >> 16) & 0xffff) as i16 as i32;
            let (w, h) = client(hwnd);
            let visible = layout(w, h, st.scale).visible;
            st.menu.wheel(delta / 120 * 3, visible);
            After::Redraw
        }),
        WM_LBUTTONUP | WM_RBUTTONUP => STATE.with(|s| {
            let mut b = s.borrow_mut();
            let Some(st) = b.as_mut() else { return After::Nothing };
            match hit_at(st, (lp & 0xffff) as i16 as i32, ((lp >> 16) & 0xffff) as i16 as i32) {
                Some(Hit::Row(rank)) => {
                    let id = st.apps[st.menu.shown[rank]].id.clone();
                    if msg == WM_RBUTTONUP {
                        // Clic droit : épingler / retirer.
                        start_menu::toggle_pin(&mut st.pinned, &id);
                        st.menu.refresh(&st.apps, &st.pinned);
                        After::SavePins(st.pinned.clone())
                    } else {
                        After::Launch(id)
                    }
                }
                Some(Hit::Power(p)) if msg == WM_LBUTTONUP => After::Power(p),
                Some(Hit::Prism) if msg == WM_LBUTTONUP => After::Prism,
                _ => After::Nothing,
            }
        }),
        WM_ACTIVATE => {
            // Un clic ailleurs : le menu se ferme (sans réentrer pendant l'activation).
            if wp & 0xffff == 0 {
                PostMessageW(hwnd, WM_CLOSE, 0, 0);
            }
            After::Nothing
        }
        WM_CLOSE => After::Close,
        WM_DESTROY => {
            STATE.with(|s| {
                let mut b = s.borrow_mut();
                if b.as_ref().is_some_and(|st| st.hwnd == hwnd) {
                    *b = None;
                    CLOSED_AT.with(|c| c.set(Some(Instant::now())));
                }
            });
            After::Nothing
        }
        _ => return DefWindowProcW(hwnd, msg, wp, lp),
    };
    match after {
        After::Nothing => {}
        After::Redraw => {
            InvalidateRect(hwnd, null(), 0);
        }
        After::Close => close(),
        After::Launch(id) => {
            close();
            launch(&id);
        }
        After::Power(p) => {
            close();
            power(p);
        }
        After::Prism => {
            close();
            open_prism();
        }
        After::SavePins(pins) => {
            let mut cfg = BarConfig::load();
            cfg.start_pinned = pins;
            let _ = cfg.save();
            InvalidateRect(hwnd, null(), 0);
        }
    }
    0
}
