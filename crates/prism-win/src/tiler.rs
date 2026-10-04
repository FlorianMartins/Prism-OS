//! Fenêtres en tuiles sous Windows : la géométrie vient de `prism_core::tiling`, ce
//! module choisit les fenêtres concernées et les place avec `SetWindowPos` (API
//! documentée, comme le font les gestionnaires komorebi ou FancyZones). Chaque
//! fenêtre retrouve sa place d'origine quand les tuiles sont désactivées.

use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::ptr::null_mut;
use std::time::{Duration, Instant};

use prism_core::bar::Rect;
use prism_core::tiling::{keep_order, tile, TilingConfig};
use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

/// Une fenêtre peut-elle être mise en tuile ? (Le jeu, le plein écran et les
/// exclusions sont vérifiés par l'appelant, qui connaît les règles du Mode Jeu.)
pub fn tileable_shape(hwnd: HWND) -> bool {
    // SAFETY: lectures d'attributs d'une fenêtre.
    unsafe {
        if IsWindowVisible(hwnd) == 0 || IsIconic(hwnd) != 0 || IsZoomed(hwnd) != 0 {
            return false;
        }
        if !GetWindow(hwnd, GW_OWNER).is_null() {
            return false; // boîte de dialogue, fenêtre secondaire
        }
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        // Seulement les fenêtres normales redimensionnables.
        if style & WS_CAPTION != WS_CAPTION || style & WS_THICKFRAME == 0 {
            return false;
        }
        if ex & (WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE) != 0 {
            return false;
        }
        let mut cloaked = 0u32;
        DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED as u32, &mut cloaked as *mut _ as *mut c_void, 4);
        cloaked == 0
    }
}

/// Zone de travail (hors barres des tâches) de l'écran d'une fenêtre.
fn work_area(mon: isize) -> Option<Rect> {
    // SAFETY: structure de sortie de taille renseignée.
    unsafe {
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = size_of::<MONITORINFO>() as u32;
        let ok = GetMonitorInfoW(mon as _, &mut mi) != 0;
        ok.then_some(Rect {
            left: mi.rcWork.left,
            top: mi.rcWork.top,
            right: mi.rcWork.right,
            bottom: mi.rcWork.bottom,
        })
    }
}

fn window_rect(hwnd: HWND) -> Option<RECT> {
    let mut r = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: sortie locale.
    (unsafe { GetWindowRect(hwnd, &mut r) } != 0).then_some(r)
}

/// Place la partie *visible* de la fenêtre sur `target` : Windows 10/11 ajoute des
/// bordures invisibles autour des fenêtres, qu'il faut compenser.
fn place(hwnd: HWND, target: Rect) -> bool {
    let (Some(full), Some(vis)) = (window_rect(hwnd), crate::fx_overlay::visible_rect(hwnd)) else {
        return false;
    };
    let (dl, dt) = (vis.left - full.left, vis.top - full.top);
    let (dr, db) = (full.right - vis.right, full.bottom - vis.bottom);
    if (vis.left - target.left).abs() <= 1
        && (vis.top - target.top).abs() <= 1
        && (vis.right - target.right).abs() <= 1
        && (vis.bottom - target.bottom).abs() <= 1
    {
        return false; // déjà en place
    }
    // SAFETY: déplacement d'une fenêtre d'une autre appli, sans l'activer ni changer
    // son ordre d'affichage.
    unsafe {
        // Une fenêtre agrandie ou ancrée par Windows garde sinon sa taille « ancrée ».
        if IsZoomed(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
        }
        SetWindowPos(
            hwnd,
            null_mut(),
            target.left - dl,
            target.top - dt,
            target.width() + dl + dr,
            target.height() + dt + db,
            SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
        ) != 0
    }
}

#[derive(Default)]
pub struct Tiler {
    /// Ordre des fenêtres par écran (la première est la principale).
    order: HashMap<isize, Vec<isize>>,
    /// Place d'origine de chaque fenêtre déplacée, pour la lui rendre.
    original: HashMap<isize, RECT>,
    /// Fenêtres déplacées par Prism à l'instant (pas d'effet « agrandir » pour elles).
    moved: HashMap<isize, Instant>,
    /// Fenêtres laissées flottantes à la main (raccourci).
    pub floating: HashSet<isize>,
    /// Dernière tuile demandée à chaque fenêtre et où elle s'est réellement posée :
    /// une fenêtre qui impose une taille minimale plus grande que sa tuile n'est pas
    /// redéplacée à chaque seconde (elle « lutterait »).
    placed: HashMap<isize, Rect>,
    /// Fenêtres laissées flottantes parce qu'elles ne tiennent pas dans une tuile.
    pub auto_floating: HashSet<isize>,
    /// Fenêtres candidates au dernier passage : quand l'ensemble change (une fenêtre
    /// fermée libère de la place), les flottantes « faute de place » sont réessayées.
    last_set: Vec<isize>,
    /// Diagnostic (relevé par la barre).
    pub log: Vec<String>,
}

fn window_pid(hwnd: HWND) -> u32 {
    let mut pid = 0u32;
    // SAFETY: sortie locale.
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    pid
}

impl Tiler {
    /// Au démarrage de la barre : reprend les places d'origine laissées par une barre
    /// précédente (arrêtée de force), pour la même fenêtre du même processus.
    pub fn with_saved_originals() -> Tiler {
        let mut t = Tiler::default();
        let saved = prism_core::tiling::load_originals();
        t.log.push(format!("places d'origine relues : {saved:?}"));
        for (h, pid, r) in saved {
            // SAFETY: lecture d'état.
            if unsafe { IsWindow(h as HWND) } != 0 && window_pid(h as HWND) == pid {
                t.original.insert(
                    h,
                    RECT {
                        left: r[0],
                        top: r[1],
                        right: r[2],
                        bottom: r[3],
                    },
                );
            }
        }
        t.persist();
        t
    }

    fn persist(&self) {
        let list: prism_core::tiling::Originals = self
            .original
            .iter()
            .map(|(h, r)| (*h, window_pid(*h as HWND), [r.left, r.top, r.right, r.bottom]))
            .collect();
        prism_core::tiling::save_originals(&list);
    }

    /// Prism vient-il de déplacer cette fenêtre ?
    pub fn recently_moved(&self, hwnd: HWND) -> bool {
        self.moved
            .get(&(hwnd as isize))
            .is_some_and(|t| t.elapsed() < Duration::from_millis(800))
    }

    /// Fenêtres en tuile de l'écran `mon`, dans l'ordre.
    pub fn order_of(&self, mon: isize) -> &[isize] {
        self.order.get(&mon).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn monitor_of(hwnd: HWND) -> isize {
        // SAFETY: renvoie toujours un écran.
        unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) as isize }
    }

    /// Met en tuiles `windows` (déjà filtrées) ; renvoie le nombre de fenêtres déplacées.
    pub fn apply(&mut self, windows: &[HWND], cfg: &TilingConfig, scale: f32) -> usize {
        let mut set: Vec<isize> = windows
            .iter()
            .map(|w| *w as isize)
            .filter(|w| !self.auto_floating.contains(w))
            .collect();
        set.sort_unstable();
        if set != self.last_set {
            for w in self.auto_floating.drain() {
                self.floating.remove(&w);
            }
            self.last_set = set;
        }
        let mut by_monitor: HashMap<isize, Vec<isize>> = HashMap::new();
        for w in windows {
            if self.floating.contains(&(*w as isize)) {
                continue;
            }
            by_monitor.entry(Self::monitor_of(*w)).or_default().push(*w as isize);
        }
        // Écrans sans fenêtre : on oublie leur ordre.
        self.order.retain(|m, _| by_monitor.contains_key(m));
        let gap = (cfg.gap as f32 * scale).round() as i32;
        let mut moved = 0;
        let mut overflow = Vec::new();
        for (mon, current) in by_monitor {
            let order = keep_order(self.order.get(&mon).map(Vec::as_slice).unwrap_or(&[]), &current);
            let Some(area) = work_area(mon) else { continue };
            let tiles = tile(cfg.layout, area, order.len(), gap, cfg.master_ratio());
            for (h, t) in order.iter().zip(&tiles) {
                let hwnd = *h as HWND;
                if !self.original.contains_key(h) {
                    if let Some(r) = window_rect(hwnd) {
                        self.original.insert(*h, r);
                        self.persist();
                    }
                }
                let now = crate::fx_overlay::visible_rect(hwnd);
                // Vérifié au passage suivant (une seconde plus tard) : les applis du
                // Store appliquent une nouvelle taille en différé, la lire tout de suite
                // donnerait l'ancienne.
                if let (Some(target), Some(now)) = (self.placed.get(h), now) {
                    if target == t {
                        if now.width() > t.width() + 16 || now.height() > t.height() + 16 {
                            // Taille minimale imposée par l'appli plus grande que sa
                            // tuile : elle déborderait sur les autres.
                            self.log
                                .push(format!("{h:#x} ne tient pas : tuile {t:?}, posée {now:?}"));
                            let excess = (now.width() - t.width()).max(0) as i64 * now.height() as i64
                                + (now.height() - t.height()).max(0) as i64 * now.width() as i64;
                            overflow.push((excess, hwnd));
                            continue;
                        }
                        if (now.left - t.left).abs() <= 2 && (now.top - t.top).abs() <= 2 {
                            continue; // déjà en place
                        }
                    }
                }
                self.moved.insert(*h, Instant::now());
                if place(hwnd, *t) {
                    moved += 1;
                }
                self.placed.insert(*h, *t);
            }
            self.order.insert(mon, order);
        }
        // Une seule fenêtre devient flottante à la fois, celle qui déborde le plus : les
        // autres, avec plus de place, tiennent peut-être. Puis on recommence (chaque
        // passage retire une fenêtre de plus : la boucle est bornée).
        if let Some((_, w)) = overflow.into_iter().max_by_key(|(e, _)| *e) {
            self.floating.insert(w as isize);
            self.auto_floating.insert(w as isize);
            // Rendue à sa place d'origine, qui reste notée : si elle revient en tuile
            // plus tard, c'est toujours cette place qu'on lui rendra à la fin.
            self.give_back(w);
            for order in self.order.values_mut() {
                order.retain(|x| *x != w as isize);
            }
            self.placed.remove(&(w as isize));
            moved += self.apply(windows, cfg, scale);
        }
        self.moved.retain(|_, t| t.elapsed() < Duration::from_secs(2));
        self.placed.retain(|h, _| self.order.values().any(|o| o.contains(h)));
        moved
    }

    /// Échange deux fenêtres dans l'ordre de leur écran.
    pub fn swap(&mut self, a: HWND, b: HWND) {
        let mon = Self::monitor_of(a);
        if let Some(order) = self.order.get_mut(&mon) {
            let (ia, ib) = (
                order.iter().position(|w| *w == a as isize),
                order.iter().position(|w| *w == b as isize),
            );
            if let (Some(ia), Some(ib)) = (ia, ib) {
                order.swap(ia, ib);
            }
        }
    }

    /// Place la fenêtre en tête (principale) de son écran.
    pub fn promote(&mut self, hwnd: HWND) {
        let mon = Self::monitor_of(hwnd);
        if let Some(order) = self.order.get_mut(&mon) {
            if let Some(i) = order.iter().position(|w| *w == hwnd as isize) {
                let w = order.remove(i);
                order.insert(0, w);
            }
        }
    }

    /// Fenêtre suivante (ou précédente) de l'ordre, sur l'écran de `hwnd`.
    pub fn neighbour(&self, hwnd: HWND, step: isize) -> Option<HWND> {
        let order = self.order.get(&Self::monitor_of(hwnd))?;
        let i = order.iter().position(|w| *w == hwnd as isize)? as isize;
        let n = order.len() as isize;
        Some(order[((i + step).rem_euclid(n)) as usize] as HWND)
    }

    /// Rend à chaque fenêtre sa place d'origine (désactivation, arrêt de la barre).
    pub fn restore_all(&mut self) -> usize {
        let mut n = 0;
        self.log.push(format!(
            "restauration : {:?}",
            self.original
                .iter()
                .map(|(h, r)| (*h, r.left, r.top, r.right - r.left, r.bottom - r.top))
                .collect::<Vec<_>>()
        ));
        for (h, r) in self.original.drain() {
            let hwnd = h as HWND;
            // SAFETY: lectures d'état puis déplacement sans activation.
            unsafe {
                if IsWindow(hwnd) == 0 || IsIconic(hwnd) != 0 || IsZoomed(hwnd) != 0 {
                    continue;
                }
                self.moved.insert(h, Instant::now());
                SetWindowPos(
                    hwnd,
                    null_mut(),
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
                );
            }
            n += 1;
        }
        self.order.clear();
        self.placed.clear();
        self.persist();
        n
    }

    /// La fenêtre n'est plus en tuile (fermée, devenue flottante…) : sa place
    /// d'origine est oubliée ou rendue.
    /// Remet la fenêtre à sa place d'origine (sans l'oublier).
    fn give_back(&mut self, hwnd: HWND) {
        if let Some(r) = self.original.get(&(hwnd as isize)).copied() {
            self.moved.insert(hwnd as isize, Instant::now());
            // SAFETY: déplacement sans activation.
            unsafe {
                SetWindowPos(
                    hwnd,
                    null_mut(),
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
                );
            }
        }
    }

    pub fn release(&mut self, hwnd: HWND, give_back: bool) {
        let h = hwnd as isize;
        if let Some(r) = self.original.remove(&h) {
            if give_back {
                // SAFETY: déplacement sans activation.
                unsafe {
                    SetWindowPos(
                        hwnd,
                        null_mut(),
                        r.left,
                        r.top,
                        r.right - r.left,
                        r.bottom - r.top,
                        SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
                    );
                }
            }
        }
        for order in self.order.values_mut() {
            order.retain(|w| *w != h);
        }
        self.persist();
    }
}
