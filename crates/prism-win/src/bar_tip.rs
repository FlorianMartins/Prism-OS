//! Bulle d'information de la Prism Bar : le nom (et le détail) de l'élément survolé,
//! puisque la barre montre des icônes. Fenêtre qui ne prend jamais le focus et laisse
//! passer les clics, dans les couleurs du thème, coins arrondis de Windows 11.

use std::cell::Cell;
use std::ptr::{null, null_mut};

use prism_core::bar::Edge;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, DeleteObject, EndPaint, GetDC, GetTextExtentPoint32W, InvalidateRect, ReleaseDC, SelectObject,
    SetBkMode, DT_CENTER, HGDIOBJ, PAINTSTRUCT, TRANSPARENT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, RegisterClassExW, SetWindowPos, ShowWindow, HWND_TOPMOST, SWP_NOACTIVATE, SW_HIDE,
    SW_SHOWNOACTIVATE, WM_ERASEBKGND, WM_PAINT, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

use crate::bar_app::{colors, fill, make_font, text, wide};

const CLASS: &str = "PrismBarTip";

thread_local! {
    static TIP: Cell<isize> = const { Cell::new(0) };
    static TEXT: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
    static SCALE: Cell<f32> = const { Cell::new(1.0) };
}

fn window() -> HWND {
    let h = TIP.with(|t| t.get());
    if h != 0 {
        return h as HWND;
    }
    let class = wide(CLASS);
    // SAFETY: classe enregistrée une fois, fenêtre de ce fil.
    unsafe {
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: GetModuleHandleW(null()),
            hIcon: null_mut(),
            hCursor: null_mut(),
            hbrBackground: null_mut(),
            lpszMenuName: null(),
            lpszClassName: class.as_ptr(),
            hIconSm: null_mut(),
        };
        RegisterClassExW(&wc);
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_LAYERED,
            class.as_ptr(),
            null(),
            WS_POPUP,
            0,
            0,
            10,
            10,
            null_mut(),
            null_mut(),
            GetModuleHandleW(null()),
            null(),
        );
        if !hwnd.is_null() {
            // Opaque mais transparente aux clics (WS_EX_LAYERED + TRANSPARENT).
            windows_sys::Win32::UI::WindowsAndMessaging::SetLayeredWindowAttributes(hwnd, 0, 245, 2);
            let round: u32 = 2;
            DwmSetWindowAttribute(hwnd, 33, &round as *const u32 as *const _, 4);
        }
        TIP.with(|t| t.set(hwnd as isize));
        hwnd
    }
}

/// Montre `s` au-dessus (ou à côté) de `anchor` (rectangle écran de l'élément).
pub fn show(s: &str, anchor: RECT, edge: Edge, scale: f32) {
    let hwnd = window();
    if hwnd.is_null() {
        return;
    }
    TEXT.with(|t| *t.borrow_mut() = s.to_string());
    SCALE.with(|c| c.set(scale));
    let px = (13.0 * scale) as i32;
    // SAFETY: mesure du texte dans le DC de l'écran, police créée puis détruite.
    let (w, h) = unsafe {
        let dc = GetDC(null_mut());
        let font = make_font(px, 400);
        let old = SelectObject(dc, font as HGDIOBJ);
        let wtext: Vec<u16> = s.encode_utf16().collect();
        let mut size = SIZE { cx: 0, cy: 0 };
        GetTextExtentPoint32W(dc, wtext.as_ptr(), wtext.len() as i32, &mut size);
        SelectObject(dc, old);
        DeleteObject(font as HGDIOBJ);
        ReleaseDC(null_mut(), dc);
        (size.cx + (24.0 * scale) as i32, size.cy + (14.0 * scale) as i32)
    };
    let gap = (8.0 * scale) as i32;
    let cx = (anchor.left + anchor.right) / 2;
    let cy = (anchor.top + anchor.bottom) / 2;
    let (x, y) = match edge {
        Edge::Bottom => (cx - w / 2, anchor.top - h - gap),
        Edge::Top => (cx - w / 2, anchor.bottom + gap),
        Edge::Left => (anchor.right + gap, cy - h / 2),
        Edge::Right => (anchor.left - w - gap, cy - h / 2),
    };
    // SAFETY: notre fenêtre.
    unsafe {
        SetWindowPos(hwnd, HWND_TOPMOST, x.max(0), y.max(0), w, h, SWP_NOACTIVATE);
        InvalidateRect(hwnd, null(), 1);
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
}

pub fn hide() {
    let h = TIP.with(|t| t.get());
    if h != 0 {
        // SAFETY: notre fenêtre.
        unsafe { ShowWindow(h as HWND, SW_HIDE) };
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut r: RECT = std::mem::zeroed();
            windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut r);
            let c = colors();
            fill(hdc, r, c.item);
            SetBkMode(hdc, TRANSPARENT as i32);
            let scale = SCALE.with(|s| s.get());
            let font = make_font((13.0 * scale) as i32, 400);
            TEXT.with(|t| text(hdc, font, c.text, r, &t.borrow(), DT_CENTER));
            DeleteObject(font as HGDIOBJ);
            EndPaint(hwnd, &ps);
            0
        }
        WM_ERASEBKGND => 1,
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
