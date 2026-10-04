//! Prism Bar : barre native Win32 (GDI), ancrée par `SHAppBarMessage` sur le bord
//! choisi. Légère par construction : elle tourne en permanence.

use std::cell::RefCell;
use std::ffi::c_void;
use std::ptr::{null, null_mut};

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
const BG: COLORREF = rgb(0x12, 0x17, 0x1e);
const ITEM: COLORREF = rgb(0x1f, 0x27, 0x32);
const ITEM_ACTIVE: COLORREF = rgb(0x1d, 0x3d, 0x47);
const TEXT: COLORREF = rgb(0xe6, 0xed, 0xf3);
const MUTED: COLORREF = rgb(0x8b, 0x96, 0xa3);
const ACCENT: COLORREF = rgb(0x5c, 0xcf, 0xe6);
const OK: COLORREF = rgb(0x57, 0xd9, 0xa3);
const WARN: COLORREF = rgb(0xe8, 0xb3, 0x4b);

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
    items: Vec<Placed>,
    game: bool,
    hidden_for_fullscreen: bool,
    /// État de la barre Windows avant que Prism la masque (pour le remettre).
    taskbar_prev: Option<u32>,
    scale: f32,
    desk: Vec<DeskWin>,
    /// Règles du Mode Jeu, pour ne jamais rendre un jeu transparent.
    game_cfg: prism_core::config::Config,
    /// Fenêtres rendues transparentes : style étendu d'origine et opacité posée.
    translucent: std::collections::HashMap<isize, (isize, u8)>,
}

thread_local! {
    static BAR: RefCell<Option<Bar>> = const { RefCell::new(None) };
}

fn with_bar<R>(f: impl FnOnce(&mut Bar) -> R) -> Option<R> {
    BAR.with(|b| b.borrow_mut().as_mut().map(f))
}

fn edge_code(e: Edge) -> u32 {
    match e {
        Edge::Top => ABE_TOP,
        Edge::Bottom => ABE_BOTTOM,
        Edge::Left => ABE_LEFT,
        Edge::Right => ABE_RIGHT,
    }
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

/// Masque automatiquement (ou remet) la barre des tâches de Windows : c'est
/// l'option « Masquer automatiquement la barre des tâches » de Windows.
fn set_taskbar_autohide(on: bool, prev: Option<u32>) -> Option<u32> {
    let tb = taskbar_hwnd();
    if tb.is_null() {
        return prev;
    }
    let mut d = appbar_data(tb);
    // SAFETY: structure locale initialisée avec sa taille.
    unsafe {
        let current = SHAppBarMessage(ABM_GETSTATE, &mut d) as u32;
        let target = if on {
            ABS_AUTOHIDE
        } else {
            prev.unwrap_or(current & !ABS_AUTOHIDE)
        };
        d.lParam = target as LPARAM;
        SHAppBarMessage(ABM_SETSTATE, &mut d);
        Some(current)
    }
}

impl Bar {
    fn scaled(&self, v: u32) -> i32 {
        (v as f32 * self.scale).round() as i32
    }

    /// (Re)place la barre : réserve la bande auprès du shell puis s'y positionne.
    fn dock(&mut self) {
        let screen = Rect {
            left: 0,
            top: 0,
            // SAFETY: lecture de métriques système.
            right: unsafe { GetSystemMetrics(SM_CXSCREEN) },
            bottom: unsafe { GetSystemMetrics(SM_CYSCREEN) },
        };
        let mut cfg = self.cfg.clone();
        cfg.thickness = self.scaled(cfg.thickness) as u32;
        cfg.margin = self.scaled(cfg.margin) as u32;
        let (bar, reserved) = bar_rect(screen, &cfg);
        let mut d = appbar_data(self.hwnd);
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
                self.hwnd,
                HWND_TOPMOST,
                bar.left,
                bar.top,
                bar.width(),
                bar.height(),
                SWP_NOACTIVATE,
            );
            SetLayeredWindowAttributes(self.hwnd, 0, (self.cfg.opacity as u32 * 255 / 100) as u8, LWA_ALPHA);
            // Coins arrondis (Windows 11) : DWMWA_WINDOW_CORNER_PREFERENCE = 33.
            let pref: u32 = if self.cfg.rounded { 2 } else { 1 };
            DwmSetWindowAttribute(self.hwnd, 33, &pref as *const _ as *const c_void, 4);
        }
        self.relayout();
        self.sync_desktop();
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
                        if !self.hidden_for_fullscreen {
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
            fill(mem, rc, BG);
            fill(
                mem,
                RECT {
                    left: 0,
                    top: 0,
                    right: w,
                    bottom: self.scaled(3),
                },
                ACCENT,
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
                        TEXT,
                        top,
                        &format!("{:02}:{:02}", t.wHour, t.wMinute),
                        DT_LEFT,
                    );
                    text(
                        mem,
                        font,
                        MUTED,
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
                        text(mem, small, MUTED, line, label, DT_LEFT);
                        text(mem, font, TEXT, line, shown, DT_RIGHT);
                        if let Some(v) = value {
                            let bar = RECT {
                                left: inner.left,
                                top: top + row_h / 2 + 6,
                                right: inner.right,
                                bottom: top + row_h / 2 + 10,
                            };
                            fill(mem, bar, ITEM);
                            let filled = RECT {
                                right: bar.left + ((bar.right - bar.left) as f32 * v / 100.0) as i32,
                                ..bar
                            };
                            fill(mem, filled, if *v > 85.0 { WARN } else { ACCENT });
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
                    text(mem, small, MUTED, head, label, DT_LEFT);
                    let shown = value.map(|v| format!("{v:.0} %")).unwrap_or_else(|| "—".into());
                    text(mem, big, TEXT, head, &shown, DT_RIGHT);
                    let graph = RECT {
                        top: head.bottom + self.scaled(6),
                        ..inner
                    };
                    graph_line(mem, graph, hist.values(), ACCENT);
                }
                DeskKind::Network => {
                    let half = (inner.bottom - inner.top) / 2;
                    text(
                        mem,
                        small,
                        MUTED,
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
                        OK,
                        down,
                        &format!("↓ {}", human_rate(self.sample.net_down)),
                        DT_LEFT,
                    );
                    text(
                        mem,
                        font,
                        ACCENT,
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

    fn client_size(&self) -> (i32, i32) {
        let mut r = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        // SAFETY: sortie locale.
        unsafe { GetClientRect(self.hwnd, &mut r) };
        (r.right - r.left, r.bottom - r.top)
    }

    fn relayout(&mut self) {
        let (w, h) = self.client_size();
        self.items = layout(&self.cfg, w, h, self.windows.len());
    }

    fn reload_config_if_changed(&mut self) {
        let stamp = std::fs::metadata(BarConfig::path()).and_then(|m| m.modified()).ok();
        if stamp != self.cfg_stamp {
            self.cfg_stamp = stamp;
            let new = BarConfig::load();
            if new.hide_windows_taskbar != self.cfg.hide_windows_taskbar {
                self.taskbar_prev = set_taskbar_autohide(new.hide_windows_taskbar, self.taskbar_prev);
            }
            self.cfg = new;
            self.dock();
        }
    }

    fn tick(&mut self) {
        self.reload_config_if_changed();
        self.sample = self.metrics.sample();
        self.cpu.push(self.sample.cpu);
        self.ram.push(self.sample.ram);
        if let Some(g) = self.sample.gpu {
            self.gpu.push(g);
        }
        self.game = Etat::load().is_some_and(|e| !e.game.is_empty());
        self.windows = list_windows(self.hwnd);
        self.apply_opacity_rules();
        self.relayout();
        // SAFETY: invalidation de nos propres fenêtres.
        unsafe {
            InvalidateRect(self.hwnd, null(), 0);
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

    fn click(&mut self, x: i32, y: i32) {
        let Some(p) = self.items.iter().find(|p| p.rect.contains(x, y)).cloned() else {
            return;
        };
        match p.widget {
            Widget::Start => press_win_key(),
            Widget::Windows => {
                if let Some(w) = p.index.and_then(|i| self.windows.get(i)) {
                    activate(w.hwnd);
                }
            }
            Widget::GameMode | Widget::Cpu | Widget::Ram | Widget::Gpu => open_prism_ui(),
            _ => {}
        }
    }

    fn paint(&self, hdc: HDC) {
        let (w, h) = self.client_size();
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
                BG,
            );
            SetBkMode(mem, TRANSPARENT as i32);
            let horizontal = self.cfg.edge.horizontal();
            let font = make_font(self.scaled(13), 400);
            let bold = make_font(self.scaled(15), 600);
            let small = make_font(self.scaled(11), 400);
            let fg = GetForegroundWindow();
            for p in &self.items {
                let r = inset(p.rect, 3);
                match p.widget {
                    Widget::Start => {
                        text(mem, bold, ACCENT, r, "◆", DT_CENTER);
                    }
                    Widget::Windows => {
                        if let Some(win) = p.index.and_then(|i| self.windows.get(i)) {
                            fill(mem, r, if win.hwnd == fg { ITEM_ACTIVE } else { ITEM });
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
                                fill(mem, line, ACCENT);
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
                                TEXT,
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
                        None => text(mem, small, MUTED, r, "GPU —", DT_CENTER),
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
                        text(mem, small, TEXT, r, &s, DT_CENTER);
                    }
                    Widget::GameMode => {
                        let (s, c) = if self.game {
                            ("● Jeu", ACCENT)
                        } else {
                            ("○ Jeu", MUTED)
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
                                TEXT,
                                top,
                                &format!("{:02}:{:02}", t.wHour, t.wMinute),
                                DT_CENTER,
                            );
                            text(
                                mem,
                                small,
                                MUTED,
                                bottom,
                                &format!("{:02}/{:02}/{}", t.wDay, t.wMonth, t.wYear),
                                DT_CENTER,
                            );
                        } else {
                            text(
                                mem,
                                bold,
                                TEXT,
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
    fill(hdc, g, ITEM);
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
        WARN
    } else if value > 60.0 {
        ACCENT
    } else {
        OK
    };
    if !horizontal {
        text(
            hdc,
            small,
            MUTED,
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
        MUTED,
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
    _event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _thread: u32,
    _time: u32,
) {
    if hwnd.is_null() || id_object != OBJID_WINDOW || id_child != 0 {
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
        WM_TIMER => {
            with_bar(|b| b.tick());
            0
        }
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            with_bar(|b| b.paint(hdc));
            EndPaint(hwnd, &ps);
            0
        }
        WM_ERASEBKGND => 1,
        WM_LBUTTONUP => {
            let x = (lp & 0xffff) as i16 as i32;
            let y = ((lp >> 16) & 0xffff) as i16 as i32;
            with_bar(|b| b.click(x, y));
            0
        }
        WM_APPBAR => {
            match wp as u32 {
                ABN_POSCHANGED => {
                    with_bar(|b| b.dock());
                }
                ABN_FULLSCREENAPP => {
                    // Un jeu ou une vidéo passe en plein écran : la barre s'efface.
                    with_bar(|b| {
                        if b.cfg.hide_in_fullscreen {
                            b.hidden_for_fullscreen = lp != 0;
                            let show = if lp != 0 { SW_HIDE } else { SW_SHOWNOACTIVATE };
                            ShowWindow(hwnd, show);
                            for d in &b.desk {
                                ShowWindow(d.hwnd, show);
                            }
                        }
                    });
                }
                _ => {}
            }
            0
        }
        WM_DPICHANGED => {
            with_bar(|b| {
                b.scale = GetDpiForWindow(hwnd) as f32 / 96.0;
                b.dock();
            });
            0
        }
        WM_DISPLAYCHANGE | WM_SETTINGCHANGE => {
            with_bar(|b| b.dock());
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_CLOSE | WM_ENDSESSION => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            with_bar(|b| {
                b.restore_opacity();
                for d in b.desk.drain(..) {
                    DestroyWindow(d.hwnd);
                }
                let mut d = appbar_data(b.hwnd);
                SHAppBarMessage(ABM_REMOVE, &mut d);
                if b.cfg.hide_windows_taskbar {
                    set_taskbar_autohide(false, b.taskbar_prev);
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
        let title = wide("Prism Bar");
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
            hinst,
            null(),
        );
        if hwnd.is_null() {
            return Err("CreateWindowExW a échoué".into());
        }
        let cfg = BarConfig::load();
        let mut d = appbar_data(hwnd);
        SHAppBarMessage(ABM_NEW, &mut d);
        let taskbar_prev = if cfg.hide_windows_taskbar {
            set_taskbar_autohide(true, None)
        } else {
            None
        };
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
                items: Vec::new(),
                game: false,
                hidden_for_fullscreen: false,
                taskbar_prev,
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
            })
        });
        with_bar(|b| {
            b.dock();
            b.tick();
        });
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        SetTimer(hwnd, TIMER_ID, 1000, None);
        use windows_sys::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent};
        let hook = SetWinEventHook(
            EVENT_OBJECT_SHOW,
            EVENT_OBJECT_SHOW,
            null_mut(),
            Some(on_object_show),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        );
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        if !hook.is_null() {
            UnhookWinEvent(hook);
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

/// Une Prism Bar tourne-t-elle ?
pub fn running() -> bool {
    let class = wide(CLASS);
    // SAFETY: recherche par nom de classe.
    unsafe { !FindWindowW(class.as_ptr(), null()).is_null() }
}
