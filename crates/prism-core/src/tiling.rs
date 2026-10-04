//! Gestionnaire de fenêtres en tuiles (façon komorebi / Hyprland) : pure géométrie,
//! testée sans Windows. La Prism Bar l'applique avec `SetWindowPos`, écran par écran.

use serde::{Deserialize, Serialize};

use crate::bar::Rect;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Layout {
    /// Une fenêtre principale à gauche, les autres empilées à droite.
    MasterStack,
    /// Colonnes de même largeur.
    Columns,
    /// Découpe binaire en spirale (l'espace restant est coupé en deux à chaque fenêtre).
    Spiral,
    /// Une seule fenêtre visible en plein (les autres derrière, même place).
    Monocle,
}

impl Layout {
    pub const ALL: [Layout; 4] = [Layout::MasterStack, Layout::Columns, Layout::Spiral, Layout::Monocle];

    pub fn label(self) -> &'static str {
        match self {
            Layout::MasterStack => "Principale + pile",
            Layout::Columns => "Colonnes",
            Layout::Spiral => "Spirale",
            Layout::Monocle => "Monocle",
        }
    }

    pub fn next(self) -> Layout {
        let i = Layout::ALL.iter().position(|l| *l == self).unwrap_or(0);
        Layout::ALL[(i + 1) % Layout::ALL.len()]
    }
}

/// Réglages (dans `bar.json`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TilingConfig {
    pub enabled: bool,
    pub layout: Layout,
    /// Espace entre les fenêtres et autour, en pixels logiques.
    pub gap: u32,
    /// Part de la fenêtre principale (Principale + pile), en pourcentage (20 à 80).
    pub master_percent: u8,
    /// Programmes jamais mis en tuiles (motifs, ex. `"*vlc.exe"`).
    pub exclude: Vec<String>,
}

impl Default for TilingConfig {
    fn default() -> Self {
        TilingConfig {
            enabled: false,
            layout: Layout::MasterStack,
            gap: 8,
            master_percent: 55,
            exclude: Vec::new(),
        }
    }
}

pub const GAP_MAX: u32 = 48;
pub const RATIO_MIN: f32 = 0.2;
pub const RATIO_MAX: f32 = 0.8;
pub const PERCENT_MIN: u8 = 20;
pub const PERCENT_MAX: u8 = 80;

impl TilingConfig {
    pub fn sanitized(mut self) -> TilingConfig {
        self.gap = self.gap.min(GAP_MAX);
        self.master_percent = self.master_percent.clamp(PERCENT_MIN, PERCENT_MAX);
        self
    }

    pub fn master_ratio(&self) -> f32 {
        self.master_percent as f32 / 100.0
    }

    pub fn excluded(&self, exe: &str) -> bool {
        let e = exe.to_ascii_lowercase();
        self.exclude
            .iter()
            .any(|p| crate::glob::matches(&p.to_ascii_lowercase(), &e))
    }
}

fn shrink(r: Rect, by: i32) -> Rect {
    Rect {
        left: r.left + by,
        top: r.top + by,
        right: (r.right - by).max(r.left + by + 1),
        bottom: (r.bottom - by).max(r.top + by + 1),
    }
}

/// Cadres des `n` fenêtres dans la zone de travail `area`, dans l'ordre (la première
/// est la principale). `gap` : espace entre fenêtres et autour, en pixels physiques.
pub fn tile(layout: Layout, area: Rect, n: usize, gap: i32, master_ratio: f32) -> Vec<Rect> {
    if n == 0 || area.width() <= 0 || area.height() <= 0 {
        return Vec::new();
    }
    let half = gap / 2;
    // Marge extérieure d'un demi-écart ; chaque tuile perd aussi un demi-écart, ce qui
    // fait un écart plein entre deux tuiles et autour de l'écran.
    let inner = shrink(area, half);
    let cells: Vec<Rect> = match layout {
        Layout::Monocle => vec![inner; n],
        _ if n == 1 => vec![inner],
        Layout::Columns => split_columns(inner, n),
        Layout::MasterStack => {
            let ratio = master_ratio.clamp(RATIO_MIN, RATIO_MAX);
            let mx = inner.left + (inner.width() as f32 * ratio).round() as i32;
            let master = Rect { right: mx, ..inner };
            let stack = Rect { left: mx, ..inner };
            let mut v = vec![master];
            v.extend(split_rows(stack, n - 1));
            v
        }
        Layout::Spiral => {
            let mut v = Vec::with_capacity(n);
            let mut rest = inner;
            for i in 0..n {
                if i == n - 1 {
                    v.push(rest);
                    break;
                }
                // Coupe le long du côté le plus long : tuiles jamais trop étroites.
                if rest.width() >= rest.height() {
                    let mx = rest.left + rest.width() / 2;
                    v.push(Rect { right: mx, ..rest });
                    rest = Rect { left: mx, ..rest };
                } else {
                    let my = rest.top + rest.height() / 2;
                    v.push(Rect { bottom: my, ..rest });
                    rest = Rect { top: my, ..rest };
                }
            }
            v
        }
    };
    cells.into_iter().map(|c| shrink(c, half)).collect()
}

fn split_columns(r: Rect, n: usize) -> Vec<Rect> {
    (0..n)
        .map(|i| Rect {
            left: r.left + r.width() * i as i32 / n as i32,
            right: r.left + r.width() * (i as i32 + 1) / n as i32,
            ..r
        })
        .collect()
}

fn split_rows(r: Rect, n: usize) -> Vec<Rect> {
    (0..n)
        .map(|i| Rect {
            top: r.top + r.height() * i as i32 / n as i32,
            bottom: r.top + r.height() * (i as i32 + 1) / n as i32,
            ..r
        })
        .collect()
}

/// Place d'origine des fenêtres mises en tuiles : (fenêtre, processus, cadre). Gardée
/// sur disque pour qu'une barre arrêtée de force puisse les rendre au redémarrage.
pub type Originals = Vec<(isize, u32, [i32; 4])>;

pub fn originals_path() -> std::path::PathBuf {
    crate::paths::data_dir().join("tuiles.json")
}

pub fn load_originals() -> Originals {
    std::fs::read(originals_path())
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save_originals(list: &Originals) {
    let path = originals_path();
    if list.is_empty() {
        let _ = std::fs::remove_file(&path);
    } else if let Ok(json) = serde_json::to_vec(list) {
        let _ = std::fs::create_dir_all(crate::paths::data_dir());
        let _ = std::fs::write(&path, json);
    }
}

/// Ordre des fenêtres d'un écran : celles déjà connues gardent leur place, les
/// nouvelles s'ajoutent à la fin, les fermées disparaissent.
pub fn keep_order<T: PartialEq + Clone>(previous: &[T], current: &[T]) -> Vec<T> {
    let mut out: Vec<T> = previous.iter().filter(|w| current.contains(w)).cloned().collect();
    out.extend(current.iter().filter(|w| !previous.contains(w)).cloned());
    out
}

/// Index de la tuile qui contient le point (x, y) (fenêtre déposée sur une autre).
pub fn tile_at(tiles: &[Rect], x: i32, y: i32) -> Option<usize> {
    tiles.iter().position(|t| t.contains(x, y))
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Rect = Rect {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1040,
    };

    fn overlap(a: &Rect, b: &Rect) -> bool {
        a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom
    }

    #[test]
    fn every_layout_fills_the_area_without_overlap() {
        for layout in [Layout::MasterStack, Layout::Columns, Layout::Spiral] {
            for n in 1..=7 {
                let t = tile(layout, AREA, n, 0, 0.55);
                assert_eq!(t.len(), n, "{layout:?} {n}");
                let area: i64 = t.iter().map(|r| r.width() as i64 * r.height() as i64).sum();
                assert_eq!(area, 1920 * 1040, "{layout:?} {n} : toute la zone est couverte");
                for i in 0..n {
                    for j in i + 1..n {
                        assert!(!overlap(&t[i], &t[j]), "{layout:?} {n} : {i} et {j}");
                    }
                }
            }
        }
    }

    #[test]
    fn gaps_separate_windows_and_screen_edges() {
        let t = tile(Layout::MasterStack, AREA, 3, 10, 0.5);
        assert_eq!((t[0].left, t[0].top), (10, 10), "écart au bord");
        assert_eq!(t[1].left - t[0].right, 10, "écart entre principale et pile");
        assert_eq!(t[2].top - t[1].bottom, 10, "écart dans la pile");
        assert_eq!(AREA.right - t[1].right, 10);
        assert_eq!(AREA.bottom - t[2].bottom, 10);
    }

    #[test]
    fn master_ratio_is_respected_and_clamped() {
        let t = tile(Layout::MasterStack, AREA, 2, 0, 0.7);
        assert_eq!(t[0].width(), 1344);
        let t = tile(Layout::MasterStack, AREA, 2, 0, 0.99);
        assert_eq!(t[0].width(), (1920.0 * RATIO_MAX) as i32);
    }

    #[test]
    fn monocle_and_single_window_take_the_whole_area() {
        assert!(tile(Layout::Monocle, AREA, 4, 0, 0.5).iter().all(|r| *r == AREA));
        assert_eq!(tile(Layout::Columns, AREA, 1, 0, 0.5), vec![AREA]);
        assert!(tile(Layout::Spiral, AREA, 0, 0, 0.5).is_empty());
    }

    #[test]
    fn spiral_cuts_along_the_longest_side() {
        let t = tile(Layout::Spiral, AREA, 3, 0, 0.5);
        assert_eq!(t[0].width(), 960, "écran large : première coupe verticale");
        assert_eq!(t[1].height(), 520, "puis horizontale dans la moitié restante");
    }

    #[test]
    fn order_is_stable_and_tiles_can_be_found_by_point() {
        assert_eq!(keep_order(&[1, 2, 3], &[3, 4, 1]), vec![1, 3, 4]);
        let t = tile(Layout::Columns, AREA, 3, 0, 0.5);
        assert_eq!(tile_at(&t, 1000, 500), Some(1));
        assert_eq!(tile_at(&t, 5000, 500), None);
    }

    #[test]
    fn config_is_sanitized_and_old_files_load() {
        let c = TilingConfig {
            gap: 500,
            master_percent: 99,
            ..Default::default()
        }
        .sanitized();
        assert_eq!((c.gap, c.master_percent), (GAP_MAX, PERCENT_MAX));
        let c: TilingConfig = serde_json::from_str("{}").unwrap();
        assert!(!c.enabled, "désactivé par défaut");
        let c = TilingConfig {
            exclude: vec!["*vlc.exe".into()],
            ..Default::default()
        };
        assert!(c.excluded("VLC.exe") && !c.excluded("notepad.exe"));
    }
}
