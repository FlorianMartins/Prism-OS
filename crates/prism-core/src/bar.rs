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
    /// Zone système : Paramètres rapides (volume, réseau), notifications, icônes des
    /// applis (barre Windows montrée quelques secondes).
    Tray,
    Clock,
}

impl Widget {
    pub const ALL: [Widget; 9] = [
        Widget::Start,
        Widget::Windows,
        Widget::Cpu,
        Widget::Ram,
        Widget::Gpu,
        Widget::Network,
        Widget::GameMode,
        Widget::Tray,
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
            Widget::Tray => "Zone système",
            Widget::Clock => "Horloge",
        }
    }
}

/// Widget posé sur le bureau (façon Conky / Rainmeter).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeskKind {
    Clock,
    System,
    Cpu,
    Ram,
    Gpu,
    Network,
}

impl DeskKind {
    pub const ALL: [DeskKind; 6] = [
        DeskKind::Clock,
        DeskKind::System,
        DeskKind::Cpu,
        DeskKind::Ram,
        DeskKind::Gpu,
        DeskKind::Network,
    ];

    pub fn label(self) -> &'static str {
        match self {
            DeskKind::Clock => "Horloge",
            DeskKind::System => "Panneau système",
            DeskKind::Cpu => "Graphe processeur",
            DeskKind::Ram => "Graphe mémoire",
            DeskKind::Gpu => "Graphe GPU",
            DeskKind::Network => "Débit réseau",
        }
    }

    /// Taille logique (avant mise à l'échelle DPI).
    pub fn size(self) -> (u32, u32) {
        match self {
            DeskKind::Clock => (280, 120),
            DeskKind::System => (300, 196),
            _ => (260, 110),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopWidget {
    pub kind: DeskKind,
    pub x: i32,
    pub y: i32,
}

/// Règle de transparence d'une appli (`windowsterminal.exe` -> 90 %).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpacityRule {
    /// Nom de l'exécutable, motif en minuscules (`*` permis).
    pub process: String,
    pub opacity: u8,
}

/// En dessous, une fenêtre devient illisible.
pub const RULE_OPACITY_MIN: u8 = 30;

/// Éléments d'interface dont l'opacité se règle séparément (dans toutes les applis
/// classiques, jamais dans un jeu).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ElementKind {
    Menu,
    Dropdown,
    Tooltip,
    Dialog,
}

impl ElementKind {
    pub const ALL: [ElementKind; 4] = [
        ElementKind::Menu,
        ElementKind::Dropdown,
        ElementKind::Tooltip,
        ElementKind::Dialog,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ElementKind::Menu => "Menus et menus contextuels",
            ElementKind::Dropdown => "Listes déroulantes",
            ElementKind::Tooltip => "Infobulles",
            ElementKind::Dialog => "Boîtes de dialogue",
        }
    }

    /// Catégorie d'une fenêtre d'après sa classe Windows (classes système stables
    /// depuis Windows 95). Les menus des applis WinUI/XAML sont dessinés dans la
    /// fenêtre de l'appli : ils n'ont pas de classe propre et ne sont pas concernés.
    pub fn from_class(class: &str) -> Option<ElementKind> {
        match class {
            "#32768" => Some(ElementKind::Menu),
            "ComboLBox" => Some(ElementKind::Dropdown),
            "tooltips_class32" => Some(ElementKind::Tooltip),
            "#32770" => Some(ElementKind::Dialog),
            _ => None,
        }
    }
}

/// Opacité par catégorie d'élément, en pourcentage (100 = non touché).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ElementOpacity {
    pub menu: u8,
    pub dropdown: u8,
    pub tooltip: u8,
    pub dialog: u8,
}

impl Default for ElementOpacity {
    fn default() -> Self {
        ElementOpacity {
            menu: 100,
            dropdown: 100,
            tooltip: 100,
            dialog: 100,
        }
    }
}

impl ElementOpacity {
    pub fn get(&self, k: ElementKind) -> u8 {
        match k {
            ElementKind::Menu => self.menu,
            ElementKind::Dropdown => self.dropdown,
            ElementKind::Tooltip => self.tooltip,
            ElementKind::Dialog => self.dialog,
        }
    }

    pub fn get_mut(&mut self, k: ElementKind) -> &mut u8 {
        match k {
            ElementKind::Menu => &mut self.menu,
            ElementKind::Dropdown => &mut self.dropdown,
            ElementKind::Tooltip => &mut self.tooltip,
            ElementKind::Dialog => &mut self.dialog,
        }
    }

    /// Opacité à appliquer à une fenêtre de cette classe (`None` : ne pas toucher).
    pub fn for_class(&self, class: &str) -> Option<u8> {
        ElementKind::from_class(class).map(|k| self.get(k)).filter(|o| *o < 100)
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
    /// Widgets posés sur le bureau.
    pub desktop_widgets: Vec<DesktopWidget>,
    /// Opacité des widgets du bureau, en pourcentage.
    pub desktop_opacity: u8,
    /// Transparence par appli (jamais appliquée à un jeu ni à une fenêtre plein écran).
    pub opacity_rules: Vec<OpacityRule>,
    /// Transparence par type d'élément (menus, listes, infobulles, dialogues).
    pub element_opacity: ElementOpacity,
    /// Effets de fenêtres (génie, gélatine, zoom) joués par la barre.
    pub fx: crate::fx::FxConfig,
    /// Une barre sur chaque écran (sinon : l'écran principal seulement).
    pub all_monitors: bool,
    /// Chaque barre ne montre que les fenêtres de son écran (sinon : toutes).
    pub windows_per_monitor: bool,
    /// Thème de couleurs (barre, widgets, appli).
    pub theme: crate::theme::ThemeConfig,
    /// Fenêtres en tuiles.
    pub tiling: crate::tiling::TilingConfig,
    /// Le bouton Démarrer ouvre le menu de Prism (sinon celui de Windows).
    pub prism_start_menu: bool,
    /// Applis épinglées en haut du menu Démarrer de Prism (identifiants `shell:AppsFolder`).
    pub start_pinned: Vec<String>,
    /// La zone système a déjà été ajoutée une fois aux barres existantes (migration).
    /// Absent d'un ancien `bar.json` : faux (la valeur par défaut de la structure, vraie
    /// pour une barre neuve, ne doit pas s'appliquer ici).
    #[serde(default)]
    pub tray_added: bool,
}

impl BarConfig {
    /// Barre configurée avant l'arrivée de la zone système : elle est ajoutée une fois,
    /// avant l'horloge (sans elle, la barre Windows masquée rend le volume, le réseau et
    /// les icônes des applis inaccessibles). Retirée ensuite, elle ne revient pas.
    pub fn migrate(&mut self) -> bool {
        if self.tray_added {
            return false;
        }
        self.tray_added = true;
        if !self.widgets.contains(&Widget::Tray) {
            let at = self
                .widgets
                .iter()
                .position(|w| *w == Widget::Clock)
                .unwrap_or(self.widgets.len());
            self.widgets.insert(at, Widget::Tray);
        }
        true
    }
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
                Widget::Tray,
                Widget::Clock,
            ],
            desktop_widgets: Vec::new(),
            desktop_opacity: 85,
            opacity_rules: Vec::new(),
            element_opacity: ElementOpacity::default(),
            fx: crate::fx::FxConfig::default(),
            all_monitors: true,
            windows_per_monitor: true,
            theme: crate::theme::ThemeConfig::default(),
            tiling: crate::tiling::TilingConfig::default(),
            prism_start_menu: true,
            start_pinned: Vec::new(),
            tray_added: true,
        }
    }
}

/// Fenêtres montrées par chaque barre. `window_monitor[i]` : écran de la fenêtre i ;
/// `panel_monitor[p]` : écran de la barre p (la barre 0 est celle de l'écran principal).
/// Une fenêtre sur un écran sans barre va sur la barre principale : aucune n'est perdue.
pub fn panel_windows(window_monitor: &[usize], panel_monitor: &[usize], per_monitor: bool) -> Vec<Vec<usize>> {
    let mut out = vec![Vec::new(); panel_monitor.len()];
    if out.is_empty() {
        return out;
    }
    for (i, m) in window_monitor.iter().enumerate() {
        if per_monitor {
            let p = panel_monitor.iter().position(|pm| pm == m).unwrap_or(0);
            out[p].push(i);
        } else {
            for list in out.iter_mut() {
                list.push(i);
            }
        }
    }
    out
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
        self.desktop_opacity = self.desktop_opacity.clamp(OPACITY_MIN, 100);
        self.tiling = self.tiling.sanitized();
        let mut kinds = Vec::new();
        self.desktop_widgets.retain(|w| {
            let fresh = !kinds.contains(&w.kind);
            kinds.push(w.kind);
            fresh
        });
        for w in &mut self.desktop_widgets {
            w.x = w.x.clamp(-16_000, 16_000);
            w.y = w.y.clamp(-16_000, 16_000);
        }
        for r in &mut self.opacity_rules {
            r.process = r.process.trim().to_lowercase();
            r.opacity = r.opacity.clamp(RULE_OPACITY_MIN, 100);
        }
        self.fx = self.fx.clone().sanitized();
        for k in ElementKind::ALL {
            let o = self.element_opacity.get_mut(k);
            *o = (*o).clamp(RULE_OPACITY_MIN, 100);
        }
        let mut names: Vec<String> = Vec::new();
        self.opacity_rules.retain(|r| {
            let fresh = !r.process.is_empty() && !names.contains(&r.process);
            names.push(r.process.clone());
            fresh
        });
        self
    }

    /// Ajoute un widget du bureau en cascade (sous les précédents), ou le retire.
    pub fn toggle_desktop(&mut self, kind: DeskKind, on: bool) {
        if on {
            if !self.desktop_widgets.iter().any(|w| w.kind == kind) {
                let y = 60
                    + self
                        .desktop_widgets
                        .iter()
                        .map(|w| w.kind.size().1 as i32 + 16)
                        .sum::<i32>();
                self.desktop_widgets.push(DesktopWidget { kind, x: 60, y });
            }
        } else {
            self.desktop_widgets.retain(|w| w.kind != kind);
        }
    }

    /// Opacité voulue pour un exécutable, s'il a une règle (première qui correspond).
    pub fn rule_for(&self, exe: &str) -> Option<u8> {
        let exe = exe.to_lowercase();
        self.opacity_rules
            .iter()
            .find(|r| crate::glob::matches(&r.process, &exe))
            .map(|r| r.opacity)
    }

    pub fn path() -> PathBuf {
        crate::paths::user_dir().join("bar.json")
    }

    pub fn load() -> BarConfig {
        fs::read(Self::path())
            .ok()
            .and_then(|b| Self::parse(&b))
            .unwrap_or_default()
            .sanitized()
    }

    /// Lecture tolérante à la marque UTF-8 qu'ajoutent PowerShell 5 et certains
    /// éditeurs Windows (sinon la barre retombait silencieusement sur ses réglages
    /// par défaut, effets coupés : vu en VM).
    pub fn parse(bytes: &[u8]) -> Option<BarConfig> {
        let b = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
        serde_json::from_slice::<BarConfig>(b).ok()
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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
        Widget::Tray => 108,
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

    #[test]
    fn tray_is_added_once_to_existing_bars() {
        let mut c: BarConfig = serde_json::from_str(r#"{"widgets":["start","windows","clock"]}"#).unwrap();
        assert!(c.migrate());
        assert_eq!(c.widgets, [Widget::Start, Widget::Windows, Widget::Tray, Widget::Clock]);
        c.widgets.retain(|w| *w != Widget::Tray);
        assert!(!c.migrate());
        assert!(!c.widgets.contains(&Widget::Tray));
        assert!(!BarConfig::default().migrate());
    }

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
    fn old_config_files_without_desktop_widgets_still_load() {
        let old = r#"{"edge":"top","thickness":36,"margin":0,"opacity":90,"rounded":false,
            "hide_windows_taskbar":true,"hide_in_fullscreen":true,"widgets":["start","clock"]}"#;
        let cfg: BarConfig = serde_json::from_str(old).unwrap();
        assert_eq!(cfg.edge, Edge::Top);
        assert!(cfg.desktop_widgets.is_empty());
        assert_eq!(cfg.desktop_opacity, 85);
    }

    #[test]
    fn desktop_widgets_cascade_and_are_unique() {
        let mut cfg = BarConfig::default();
        cfg.toggle_desktop(DeskKind::Clock, true);
        cfg.toggle_desktop(DeskKind::System, true);
        cfg.toggle_desktop(DeskKind::Clock, true);
        assert_eq!(cfg.desktop_widgets.len(), 2);
        assert!(cfg.desktop_widgets[1].y > cfg.desktop_widgets[0].y + DeskKind::Clock.size().1 as i32);
        cfg.toggle_desktop(DeskKind::Clock, false);
        assert_eq!(
            cfg.desktop_widgets.iter().map(|w| w.kind).collect::<Vec<_>>(),
            vec![DeskKind::System]
        );
    }

    #[test]
    fn opacity_rules_are_normalised_and_matched() {
        let cfg = BarConfig {
            opacity_rules: vec![
                OpacityRule {
                    process: " WindowsTerminal.exe ".into(),
                    opacity: 90,
                },
                OpacityRule {
                    process: "windowsterminal.exe".into(),
                    opacity: 50,
                },
                OpacityRule {
                    process: "code*.exe".into(),
                    opacity: 5,
                },
                OpacityRule {
                    process: "".into(),
                    opacity: 80,
                },
            ],
            ..Default::default()
        }
        .sanitized();
        assert_eq!(cfg.opacity_rules.len(), 2, "doublon et règle vide retirés");
        assert_eq!(cfg.rule_for("WindowsTerminal.exe"), Some(90));
        assert_eq!(
            cfg.rule_for("code - insiders.exe"),
            Some(RULE_OPACITY_MIN),
            "jamais illisible"
        );
        assert_eq!(cfg.rule_for("notepad.exe"), None);
    }

    #[test]
    fn element_opacity_is_per_category_and_untouched_at_100() {
        let mut cfg = BarConfig::default();
        assert_eq!(
            cfg.element_opacity.for_class("#32768"),
            None,
            "100 % : on ne touche à rien"
        );
        cfg.element_opacity.menu = 80;
        cfg.element_opacity.tooltip = 5;
        let cfg = cfg.sanitized();
        assert_eq!(cfg.element_opacity.for_class("#32768"), Some(80));
        assert_eq!(
            cfg.element_opacity.for_class("tooltips_class32"),
            Some(RULE_OPACITY_MIN)
        );
        assert_eq!(cfg.element_opacity.for_class("ComboLBox"), None);
        assert_eq!(
            cfg.element_opacity.for_class("Chrome_WidgetWin_1"),
            None,
            "fenêtre ordinaire"
        );
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

    #[test]
    fn config_with_a_utf8_bom_still_loads() {
        let mut json = b"\xef\xbb\xbf".to_vec();
        json.extend(br#"{"fx":{"enabled":true,"minimize":"fall_apart"}}"#);
        let c = BarConfig::parse(&json).expect("marque UTF-8 acceptée");
        assert!(c.fx.enabled);
        assert_eq!(c.fx.minimize, crate::fx::Effect::FallApart);
    }

    #[test]
    fn windows_are_shared_between_monitor_bars() {
        // Fenêtres sur les écrans 0, 1, 1, 2 ; barres sur les écrans 0 et 1 seulement.
        let w = [0, 1, 1, 2];
        assert_eq!(panel_windows(&w, &[0, 1], true), vec![vec![0, 3], vec![1, 2]]);
        assert_eq!(
            panel_windows(&w, &[0, 1], false),
            vec![vec![0, 1, 2, 3], vec![0, 1, 2, 3]]
        );
        // Une seule barre : tout y est.
        assert_eq!(panel_windows(&w, &[0], true), vec![vec![0, 1, 2, 3]]);
        assert!(panel_windows(&w, &[], true).is_empty());
        // Anciens réglages sans ces champs : barre partout, fenêtres par écran.
        let c = BarConfig::parse(br#"{"edge":"top"}"#).unwrap();
        assert!(c.all_monitors && c.windows_per_monitor);
    }
}
