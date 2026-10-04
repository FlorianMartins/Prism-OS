//! Prism Bar : configuration et disposition (logique pure, testée sous Linux).
//!
//! La barre elle-même est une fenêtre native Win32 (`prism-bar.exe`), ancrée par
//! l'API officielle des barres d'application (`SHAppBarMessage`) sur le bord choisi.
//! Windows 11 ne sait plus déplacer sa propre barre des tâches : Prism fournit la sienne.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Edge {
    Top,
    Bottom,
    Left,
    Right,
}

impl Edge {
    pub const ALL: [Edge; 4] = [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right];

    pub fn label(self) -> &'static str {
        match self {
            Edge::Top => "Haut",
            Edge::Bottom => "Bas",
            Edge::Left => "Gauche",
            Edge::Right => "Droite",
        }
    }

    pub fn horizontal(self) -> bool {
        matches!(self, Edge::Top | Edge::Bottom)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Widget {
    Start,
    Windows,
    Cpu,
    Ram,
    Gpu,
    Network,
    GameMode,
    Clock,
}

impl Widget {
    pub const ALL: [Widget; 8] = [
        Widget::Start,
        Widget::Windows,
        Widget::Cpu,
        Widget::Ram,
        Widget::Gpu,
        Widget::Network,
        Widget::GameMode,
        Widget::Clock,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Widget::Start => "Bouton Démarrer",
            Widget::Windows => "Fenêtres ouvertes",
            Widget::Cpu => "Processeur",
            Widget::Ram => "Mémoire",
            Widget::Gpu => "Carte graphique",
            Widget::Network => "Réseau",
            Widget::GameMode => "Mode Jeu",
            Widget::Clock => "Horloge",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BarConfig {
    pub edge: Edge,
    /// Épaisseur en pixels (hauteur si horizontale, largeur si verticale).
    pub thickness: u32,
    /// Écart avec le bord de l'écran : 0 = collée, sinon barre « flottante ».
    pub margin: u32,
    /// Opacité en pourcentage.
    pub opacity: u8,
    pub rounded: bool,
    /// Masque automatiquement la barre des tâches de Windows tant que Prism Bar tourne.
    pub hide_windows_taskbar: bool,
    /// Se cache quand un jeu ou une vidéo passe en plein écran.
    pub hide_in_fullscreen: bool,
    /// Éléments affichés, dans l'ordre.
    pub widgets: Vec<Widget>,
}

impl Default for BarConfig {
    fn default() -> Self {
        BarConfig {
            edge: Edge::Bottom,
            thickness: 40,
            margin: 0,
            opacity: 92,
            rounded: false,
            hide_windows_taskbar: true,
            hide_in_fullscreen: true,
            widgets: vec![
                Widget::Start,
                Widget::Windows,
                Widget::Cpu,
                Widget::Ram,
                Widget::GameMode,
                Widget::Clock,
            ],
        }
    }
}

pub const THICKNESS_MIN: u32 = 24;
pub const THICKNESS_MAX: u32 = 96;
pub const OPACITY_MIN: u8 = 40;
pub const MARGIN_MAX: u32 = 32;

impl BarConfig {
    /// Ramène chaque valeur dans ses bornes (un fichier modifié à la main ne peut pas
    /// produire une barre invisible ou géante).
    pub fn sanitized(mut self) -> BarConfig {
        self.thickness = self.thickness.clamp(THICKNESS_MIN, THICKNESS_MAX);
        self.opacity = self.opacity.clamp(OPACITY_MIN, 100);
        self.margin = self.margin.min(MARGIN_MAX);
        let mut seen = Vec::new();
        self.widgets.retain(|w| {
            let fresh = !seen.contains(w);
            seen.push(*w);
            fresh
        });
        self
    }

    pub fn path() -> PathBuf {
        crate::paths::data_dir().join("bar.json")
    }

    pub fn load() -> BarConfig {
        fs::read(Self::path())
            .ok()
            .and_then(|b| serde_json::from_slice::<BarConfig>(&b).ok())
            .unwrap_or_default()
            .sanitized()
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        fs::write(
            &path,
            serde_json::to_vec_pretty(&self.clone().sanitized()).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())
    }
}

/// Rectangle en pixels (gauche, haut, droite, bas).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }
    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }
}

/// Où placer la barre sur un écran, et quelle bande réserver aux autres fenêtres
/// (la barre plus sa marge, pour que rien ne passe dessous).
pub fn bar_rect(monitor: Rect, cfg: &BarConfig) -> (Rect, Rect) {
    let t = cfg.thickness as i32;
    let m = cfg.margin as i32;
    let (bar, reserved) = match cfg.edge {
        Edge::Top => (
            Rect {
                left: monitor.left + m,
                top: monitor.top + m,
                right: monitor.right - m,
                bottom: monitor.top + m + t,
            },
            Rect {
                bottom: monitor.top + 2 * m + t,
                ..monitor
            },
        ),
        Edge::Bottom => (
            Rect {
                left: monitor.left + m,
                top: monitor.bottom - m - t,
                right: monitor.right - m,
                bottom: monitor.bottom - m,
            },
            Rect {
                top: monitor.bottom - 2 * m - t,
                ..monitor
            },
        ),
        Edge::Left => (
            Rect {
                left: monitor.left + m,
                top: monitor.top + m,
                right: monitor.left + m + t,
                bottom: monitor.bottom - m,
            },
            Rect {
                right: monitor.left + 2 * m + t,
                ..monitor
            },
        ),
        Edge::Right => (
            Rect {
                left: monitor.right - m - t,
                top: monitor.top + m,
                right: monitor.right - m,
                bottom: monitor.bottom - m,
            },
            Rect {
                left: monitor.right - 2 * m - t,
                ..monitor
            },
        ),
    };
    (bar, reserved)
}

/// Élément placé dans la barre (coordonnées relatives à la barre).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placed {
    pub widget: Widget,
    /// Pour `Widget::Windows` : index de la fenêtre dans la liste fournie.
    pub index: Option<usize>,
    pub rect: Rect,
}

/// Taille d'un élément le long de la barre.
fn extent(w: Widget, thickness: i32, horizontal: bool) -> i32 {
    if !horizontal {
        return thickness; // barre verticale : éléments carrés, empilés
    }
    match w {
        Widget::Start => thickness,
        Widget::Cpu | Widget::Ram | Widget::Gpu => 92,
        Widget::Network => 120,
        Widget::GameMode => 110,
        Widget::Clock => 112,
        Widget::Windows => 0, // remplit l'espace restant
    }
}

/// Dispose les éléments : ceux avant `Windows` au début, ceux après à la fin, les
/// fenêtres ouvertes se partagent le milieu (largeur bornée, comme une barre des tâches).
pub fn layout(cfg: &BarConfig, width: i32, height: i32, window_count: usize) -> Vec<Placed> {
    let horizontal = cfg.edge.horizontal();
    let thickness = if horizontal { height } else { width };
    let length = if horizontal { width } else { height };
    let pad = 4;
    let along = |start: i32, size: i32| -> Rect {
        if horizontal {
            Rect {
                left: start,
                top: 0,
                right: start + size,
                bottom: height,
            }
        } else {
            Rect {
                left: 0,
                top: start,
                right: width,
                bottom: start + size,
            }
        }
    };
    let split = cfg.widgets.iter().position(|w| *w == Widget::Windows);
    let (head, tail): (&[Widget], &[Widget]) = match split {
        Some(i) => (&cfg.widgets[..i], &cfg.widgets[i + 1..]),
        None => (&cfg.widgets[..], &[]),
    };
    let mut out = Vec::new();
    let mut cursor = pad;
    for &w in head {
        let e = extent(w, thickness, horizontal);
        out.push(Placed {
            widget: w,
            index: None,
            rect: along(cursor, e),
        });
        cursor += e + pad;
    }
    let mut end = length - pad;
    let mut tail_placed = Vec::new();
    for &w in tail.iter().rev() {
        let e = extent(w, thickness, horizontal);
        tail_placed.push(Placed {
            widget: w,
            index: None,
            rect: along(end - e, e),
        });
        end -= e + pad;
    }
    if split.is_some() && window_count > 0 && end > cursor {
        let room = end - cursor;
        let max_item = if horizontal { 200 } else { thickness };
        let item = (room / window_count as i32 - pad).clamp(0, max_item);
        if item >= thickness.min(32) {
            for i in 0..window_count {
                let start = cursor + i as i32 * (item + pad);
                if start + item > end {
                    break;
                }
                out.push(Placed {
                    widget: Widget::Windows,
                    index: Some(i),
                    rect: along(start, item),
                });
            }
        }
    }
    tail_placed.reverse();
    out.extend(tail_placed);
    out
}

/// Historique court pour les mini-graphes (valeurs 0..=100).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct History {
    values: Vec<f32>,
    cap: usize,
}

impl History {
    pub fn new(cap: usize) -> History {
        History {
            values: Vec::with_capacity(cap),
            cap,
        }
    }
    pub fn push(&mut self, v: f32) {
        if self.values.len() == self.cap {
            self.values.remove(0);
        }
        self.values.push(v.clamp(0.0, 100.0));
    }
    pub fn values(&self) -> &[f32] {
        &self.values
    }
    pub fn last(&self) -> f32 {
        self.values.last().copied().unwrap_or(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1080,
    };

    #[test]
    fn bar_sits_on_each_edge_and_reserves_its_band() {
        let mut cfg = BarConfig {
            thickness: 40,
            ..Default::default()
        };
        for edge in Edge::ALL {
            cfg.edge = edge;
            let (bar, reserved) = bar_rect(SCREEN, &cfg);
            match edge {
                Edge::Top => assert_eq!((bar.top, bar.height(), reserved.bottom), (0, 40, 40)),
                Edge::Bottom => assert_eq!((bar.bottom, bar.height(), reserved.top), (1080, 40, 1040)),
                Edge::Left => assert_eq!((bar.left, bar.width(), reserved.right), (0, 40, 40)),
                Edge::Right => assert_eq!((bar.right, bar.width(), reserved.left), (1920, 40, 1880)),
            }
        }
    }

    #[test]
    fn floating_bar_keeps_its_margin_on_all_sides() {
        let cfg = BarConfig {
            edge: Edge::Top,
            thickness: 36,
            margin: 8,
            ..Default::default()
        };
        let (bar, reserved) = bar_rect(SCREEN, &cfg);
        assert_eq!(
            bar,
            Rect {
                left: 8,
                top: 8,
                right: 1912,
                bottom: 44
            }
        );
        assert_eq!(
            reserved.bottom, 52,
            "les fenêtres s'arrêtent sous la marge basse de la barre"
        );
    }

    #[test]
    fn values_are_clamped_and_duplicates_removed() {
        let cfg = BarConfig {
            thickness: 4,
            opacity: 0,
            margin: 500,
            widgets: vec![Widget::Clock, Widget::Clock, Widget::Cpu],
            ..Default::default()
        }
        .sanitized();
        assert_eq!(
            (cfg.thickness, cfg.opacity, cfg.margin),
            (THICKNESS_MIN, OPACITY_MIN, MARGIN_MAX)
        );
        assert_eq!(cfg.widgets, vec![Widget::Clock, Widget::Cpu]);
    }

    #[test]
    fn horizontal_layout_puts_widgets_at_both_ends_and_windows_in_between() {
        let cfg = BarConfig::default();
        let items = layout(&cfg, 1920, 40, 3);
        let start = items.iter().find(|p| p.widget == Widget::Start).unwrap();
        let clock = items.iter().find(|p| p.widget == Widget::Clock).unwrap();
        let wins: Vec<_> = items.iter().filter(|p| p.widget == Widget::Windows).collect();
        assert_eq!(start.rect.left, 4);
        assert_eq!(clock.rect.right, 1916);
        assert_eq!(wins.len(), 3);
        assert!(wins.iter().all(|w| w.rect.width() <= 200));
        assert!(wins[0].rect.left > start.rect.right);
        let cpu = items.iter().find(|p| p.widget == Widget::Cpu).unwrap();
        assert!(
            wins[2].rect.right <= cpu.rect.left,
            "les fenêtres ne recouvrent pas les widgets"
        );
        for p in &items {
            assert!(p.rect.left >= 0 && p.rect.right <= 1920 && p.rect.height() == 40);
        }
    }

    #[test]
    fn vertical_layout_stacks_square_items() {
        let cfg = BarConfig {
            edge: Edge::Left,
            thickness: 48,
            ..Default::default()
        };
        let items = layout(&cfg, 48, 1080, 2);
        for p in &items {
            assert_eq!((p.rect.width(), p.rect.height()), (48, 48), "{:?}", p.widget);
            assert!(p.rect.top >= 0 && p.rect.bottom <= 1080);
        }
        let mut tops: Vec<i32> = items.iter().map(|p| p.rect.top).collect();
        tops.sort();
        tops.dedup();
        assert_eq!(tops.len(), items.len(), "aucun chevauchement");
    }

    #[test]
    fn too_many_windows_never_overflow_the_bar() {
        let cfg = BarConfig::default();
        let items = layout(&cfg, 800, 40, 60);
        let cpu = items.iter().find(|p| p.widget == Widget::Cpu).unwrap();
        for w in items.iter().filter(|p| p.widget == Widget::Windows) {
            assert!(w.rect.right <= cpu.rect.left);
        }
    }

    #[test]
    fn history_is_bounded() {
        let mut h = History::new(3);
        for v in [10.0, 20.0, 30.0, 140.0] {
            h.push(v);
        }
        assert_eq!(h.values(), &[20.0, 30.0, 100.0]);
        assert_eq!(h.last(), 100.0);
    }
}
