//! Couche d'effets : copie d'une fenêtre (PrintWindow), animation logicielle
//! (`prism_core::fx`), affichage dans une fenêtre transparente aux clics, au rythme
//! du compositeur (DwmFlush). API officielles uniquement, aucune injection.

use std::ffi::c_void;
use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};

use prism_core::bar::Rect;
use prism_core::fx::{Animation, FxStat, Image};
use windows_sys::Win32::Foundation::{HWND, POINT, RECT, SIZE};
use windows_sys::Win32::Graphics::Dwm::{DwmFlush, DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

/// PW_RENDERFULLCONTENT : copie aussi le contenu composé (DirectX, navigateurs).
const PW_RENDERFULLCONTENT: PRINT_WINDOW_FLAGS = 2;
const CLASS: &str = "PrismFx";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Copie d'une fenêtre et sa position visible à l'écran.
pub struct Snap {
    pub img: Image,
    pub rect: Rect,
    pub at: Instant,
    pub capture_ms: f32,
}

/// Rectangle visible d'une fenêtre (sans les bordures invisibles de Windows 10/11).
pub fn visible_rect(hwnd: HWND) -> Option<Rect> {
    // SAFETY: sorties locales de taille exacte.
    unsafe {
        let mut r = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS as u32,
            &mut r as *mut _ as *mut c_void,
            16,
        ) != 0
            && GetWindowRect(hwnd, &mut r) == 0
        {
            return None;
        }
        Some(Rect {
            left: r.left,
            top: r.top,
            right: r.right,
            bottom: r.bottom,
        })
    }
}

struct Dib {
    dc: HDC,
    bmp: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u32,
    w: i32,
    h: i32,
}

impl Dib {
    fn new(w: i32, h: i32) -> Option<Dib> {
        // SAFETY: section DIB 32 bits de haut en bas, détruite par Drop.
        unsafe {
            let screen = GetDC(null_mut());
            let dc = CreateCompatibleDC(screen);
            ReleaseDC(null_mut(), screen);
            let mut bmi: BITMAPINFO = std::mem::zeroed();
            bmi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
            bmi.bmiHeader.biWidth = w;
            bmi.bmiHeader.biHeight = -h;
            bmi.bmiHeader.biPlanes = 1;
            bmi.bmiHeader.biBitCount = 32;
            bmi.bmiHeader.biCompression = BI_RGB;
            let mut bits: *mut c_void = null_mut();
            let bmp = CreateDIBSection(dc, &bmi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            if bmp.is_null() || bits.is_null() {
                DeleteDC(dc);
                return None;
            }
            let old = SelectObject(dc, bmp as HGDIOBJ);
            Some(Dib {
                dc,
                bmp,
                old,
                bits: bits as *mut u32,
                w,
                h,
            })
        }
    }

    fn pixels(&mut self) -> &mut [u32] {
        // SAFETY: la section DIB fait exactement w*h pixels 32 bits.
        unsafe { std::slice::from_raw_parts_mut(self.bits, (self.w * self.h) as usize) }
    }
}

impl Drop for Dib {
    fn drop(&mut self) {
        // SAFETY: objets créés par Dib::new.
        unsafe {
            SelectObject(self.dc, self.old);
            DeleteObject(self.bmp as HGDIOBJ);
            DeleteDC(self.dc);
        }
    }
}

/// Copie la fenêtre telle qu'elle s'affiche. `None` si Windows refuse.
pub fn capture(hwnd: HWND) -> Option<Snap> {
    let t0 = Instant::now();
    // Une fenêtre réduite reste « visible » pour Windows et reçoit même le focus,
    // mais elle est rangée hors écran en 160x28 : la copier écraserait la bonne copie.
    // SAFETY: lecture d'état.
    if unsafe { IsIconic(hwnd) } != 0 {
        return None;
    }
    let visible = visible_rect(hwnd)?;
    let mut full = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: sortie locale.
    unsafe { GetWindowRect(hwnd, &mut full) };
    let (w, h) = (full.right - full.left, full.bottom - full.top);
    if w <= 0 || h <= 0 || w > 8192 || h > 8192 || visible.width() <= 0 || visible.height() <= 0 {
        return None;
    }
    let mut dib = Dib::new(w, h)?;
    // SAFETY: le DC mémoire contient la section DIB de la taille de la fenêtre.
    if unsafe { PrintWindow(hwnd, dib.dc, PW_RENDERFULLCONTENT) } == 0 {
        return None;
    }
    // Recadrage sur la partie visible ; alpha forcé à opaque (PrintWindow ne le fournit pas).
    let (ox, oy) = (visible.left - full.left, visible.top - full.top);
    let (cw, ch) = (visible.width().min(w - ox), visible.height().min(h - oy));
    let mut img = Image::new(cw as u32, ch as u32);
    let src = dib.pixels();
    for y in 0..ch {
        let row = ((y + oy) * w + ox) as usize;
        let dst = (y * cw) as usize;
        for x in 0..cw as usize {
            img.pixels[dst + x] = src[row + x] | 0xff00_0000;
        }
    }
    Some(Snap {
        img,
        rect: visible,
        at: Instant::now(),
        capture_ms: t0.elapsed().as_secs_f32() * 1e3,
    })
}

/// Fenêtre d'affichage des effets : transparente aux clics, sans focus, au-dessus.
pub struct Overlay {
    hwnd: HWND,
    dib: Option<Dib>,
}

impl Overlay {
    pub fn new() -> Option<Overlay> {
        // SAFETY: classe et fenêtre de ce processus.
        unsafe {
            let class = wide(CLASS);
            let hinst = GetModuleHandleW(null());
            let wc = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                style: 0,
                lpfnWndProc: Some(DefWindowProcW),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: hinst,
                hIcon: null_mut(),
                hCursor: null_mut(),
                hbrBackground: null_mut(),
                lpszMenuName: null(),
                lpszClassName: class.as_ptr(),
                hIconSm: null_mut(),
            };
            RegisterClassExW(&wc);
            let title = wide("Prism Fx");
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class.as_ptr(),
                title.as_ptr(),
                WS_POPUP,
                0,
                0,
                1,
                1,
                null_mut(),
                null_mut(),
                hinst,
                null(),
            );
            (!hwnd.is_null()).then_some(Overlay { hwnd, dib: None })
        }
    }

    pub fn show_frame(&mut self, img: &Image, x: i32, y: i32) {
        let (w, h) = (img.width as i32, img.height as i32);
        if self.dib.as_ref().map_or(true, |d| d.w != w || d.h != h) {
            self.dib = Dib::new(w, h);
        }
        let Some(dib) = self.dib.as_mut() else { return };
        dib.pixels().copy_from_slice(&img.pixels);
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let pos = POINT { x, y };
        let size = SIZE { cx: w, cy: h };
        let src = POINT { x: 0, y: 0 };
        // SAFETY: section DIB valide de la taille annoncée, alpha prémultiplié.
        unsafe {
            UpdateLayeredWindow(self.hwnd, null_mut(), &pos, &size, dib.dc, &src, 0, &blend, ULW_ALPHA);
        }
    }

    /// Affiche la couche au premier plan, sans prendre le focus.
    pub fn raise(&self) {
        // SAFETY: notre fenêtre.
        unsafe {
            SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
    }

    pub fn hide(&self) {
        // SAFETY: notre fenêtre.
        unsafe { ShowWindow(self.hwnd, SW_HIDE) };
    }

    /// Joue l'animation. `after_first` s'exécute juste après la première image
    /// (ex. : réduire la vraie fenêtre une fois qu'elle est recouverte).
    #[allow(clippy::too_many_arguments)]
    pub fn play(
        &mut self,
        anim: &Animation,
        origin: (i32, i32),
        size: (u32, u32),
        duration: Duration,
        trigger: Instant,
        trigger_name: &str,
        capture_ms: f32,
        after_first: impl FnOnce(),
    ) -> FxStat {
        let mut after = Some(after_first);
        let meter = self.run(
            origin,
            size,
            duration,
            trigger,
            |t, frame| anim.render(t, frame),
            |i, _| {
                if i == 0 {
                    if let Some(f) = after.take() {
                        f();
                    }
                }
            },
        );
        if let Some(f) = after.take() {
            f();
        }
        meter.stat(
            &format!("{:?}", anim.effect),
            trigger_name,
            (anim.source.width, anim.source.height),
            capture_ms,
        )
    }

    /// Boucle d'animation générique : `render(t, image)` dessine chaque image,
    /// `on_frame(numéro, t)` s'exécute juste après son affichage.
    pub fn run(
        &mut self,
        origin: (i32, i32),
        size: (u32, u32),
        duration: Duration,
        trigger: Instant,
        mut render: impl FnMut(f32, &mut Image),
        mut on_frame: impl FnMut(u32, f32),
    ) -> Meter {
        let mut frame = Image::new(size.0, size.1);
        let start = Instant::now();
        let mut meter = Meter::new(trigger);
        loop {
            let t = start.elapsed().as_secs_f32() / duration.as_secs_f32();
            if t >= 1.0 {
                break;
            }
            let w0 = Instant::now();
            render(t, &mut frame);
            self.show_frame(&frame, origin.0, origin.1);
            if meter.frames == 0 {
                self.raise();
            }
            on_frame(meter.frames, t);
            meter.frame(w0);
        }
        self.hide();
        meter
    }
}

/// Mesure des images d'un effet : latence, travail par image, cadence, images ratées.
pub struct Meter {
    trigger: Instant,
    start: Instant,
    pub frames: u32,
    latency: f32,
    works: Vec<Duration>,
    intervals: Vec<Duration>,
    last_flip: Option<Instant>,
}

impl Meter {
    pub fn new(trigger: Instant) -> Meter {
        Meter {
            trigger,
            start: Instant::now(),
            frames: 0,
            latency: 0.0,
            works: Vec::new(),
            intervals: Vec::new(),
            last_flip: None,
        }
    }

    /// Une image vient d'être envoyée (travail commencé en `w0`) : attend la prochaine
    /// composition de l'écran, ce qui cale la cadence sur l'affichage.
    pub fn frame(&mut self, w0: Instant) {
        if self.frames == 0 {
            self.latency = self.trigger.elapsed().as_secs_f32() * 1e3;
        }
        self.works.push(w0.elapsed());
        self.frames += 1;
        // SAFETY: appel sans argument.
        unsafe { DwmFlush() };
        let now = Instant::now();
        if let Some(l) = self.last_flip {
            self.intervals.push(now - l);
        }
        self.last_flip = Some(now);
    }

    pub fn stat(&self, effect: &str, trigger: &str, size: (u32, u32), capture_ms: f32) -> FxStat {
        let elapsed = self.start.elapsed().as_secs_f32();
        let mut sorted = self.intervals.clone();
        sorted.sort();
        let refresh = sorted
            .get(sorted.len() / 2)
            .copied()
            .unwrap_or(Duration::from_micros(16_667));
        let total: Duration = self.works.iter().sum();
        FxStat {
            effect: effect.to_string(),
            trigger: trigger.to_string(),
            width: size.0,
            height: size.1,
            capture_ms,
            latency_ms: self.latency,
            frames: self.frames,
            avg_frame_ms: if self.frames > 0 {
                total.as_secs_f32() * 1e3 / self.frames as f32
            } else {
                0.0
            },
            max_frame_ms: self.works.iter().max().map_or(0.0, |w| w.as_secs_f32() * 1e3),
            missed: self.works.iter().filter(|w| **w > refresh).count() as u32,
            fps: if elapsed > 0.0 {
                self.frames as f32 / elapsed
            } else {
                0.0
            },
        }
    }
}

/// Journal des fenêtres rendues invisibles le temps d'un effet : si la barre est tuée
/// pendant ce temps, son prochain démarrage les rend visibles (`recover_hidden`).
fn hidden_update(f: impl FnOnce(&mut prism_core::fx::HiddenWindows)) {
    let mut list = prism_core::fx::load_hidden();
    f(&mut list);
    prism_core::fx::save_hidden(&list);
}

fn window_pid(hwnd: HWND) -> u32 {
    let mut pid = 0u32;
    // SAFETY: sortie locale.
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    pid
}

/// Rend une fenêtre invisible le temps d'un effet (calque presque transparent). Renvoie
/// le style d'origine, `None` si la fenêtre gère déjà sa transparence (on ne la touche pas).
pub fn hide_temp(hwnd: HWND) -> Option<isize> {
    // SAFETY: style étendu et opacité de calque d'une fenêtre, remis par `unhide`.
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        if ex as u32 & WS_EX_LAYERED != 0 {
            return None;
        }
        hidden_update(|l| l.push((hwnd as isize, window_pid(hwnd), ex)));
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | WS_EX_LAYERED as isize);
        // Opacité 1/255 et non 0 : invisible à l'œil, mais une fenêtre à opacité nulle
        // ne reçoit plus la souris, et un déplacement en cours s'arrêterait net.
        SetLayeredWindowAttributes(hwnd, 0, 1, LWA_ALPHA);
        Some(ex)
    }
}

/// Opacité d'une fenêtre cachée par `hide_temp` (fondu vers la vraie fenêtre).
pub fn set_alpha(hwnd: HWND, alpha: f32) {
    let a = (alpha.clamp(0.0, 1.0) * 255.0).round().max(1.0) as u8;
    // SAFETY: fenêtre rendue « calque » par `hide_temp`.
    unsafe { SetLayeredWindowAttributes(hwnd, 0, a, LWA_ALPHA) };
}

pub fn unhide(hwnd: HWND, ex: isize) {
    // SAFETY: remise du style étendu d'origine.
    unsafe { SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex) };
    hidden_update(|l| l.retain(|(h, _, _)| *h != hwnd as isize));
}

/// Au démarrage de la barre : rend visibles les fenêtres qu'une barre précédente,
/// arrêtée de force, avait laissées invisibles. Renvoie leur nombre.
pub fn recover_hidden() -> usize {
    let mut n = 0;
    hidden_update(|l| {
        for (h, pid, ex) in l.drain(..) {
            let hwnd = h as HWND;
            // SAFETY: lectures d'état ; même fenêtre ET même processus, sinon on ne touche à rien.
            unsafe {
                if IsWindow(hwnd) != 0
                    && window_pid(hwnd) == pid
                    && GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_LAYERED != 0
                {
                    SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex);
                    n += 1;
                }
            }
        }
    });
    n
}
