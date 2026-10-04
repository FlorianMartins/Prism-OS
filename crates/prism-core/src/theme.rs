//! Thèmes de couleurs de Prism (barre, widgets, appli), avec accent personnalisable.
//! Pure logique : la barre (GDI) et l'appli (egui) traduisent la même palette.

use serde::{Deserialize, Serialize};

pub type Rgb = [u8; 3];

/// Couleurs d'un thème.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    /// Fond des fenêtres et de la barre.
    pub bg: Rgb,
    /// Fond des panneaux (barre latérale de l'appli).
    pub panel: Rgb,
    /// Cartes, boutons de la barre.
    pub card: Rgb,
    /// Cartes en relief, boutons.
    pub card_hi: Rgb,
    pub border: Rgb,
    pub text: Rgb,
    pub muted: Rgb,
    pub accent: Rgb,
    /// Accent atténué (sélection, fenêtre active) : dérivé de l'accent et du fond.
    pub accent_dim: Rgb,
    pub ok: Rgb,
    pub warn: Rgb,
    pub bad: Rgb,
}

/// Thème choisi (dans `bar.json`, lu par la barre et par l'appli).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ThemeConfig {
    /// Identifiant d'un préréglage (`PRESETS`).
    pub preset: String,
    /// Accent personnalisé (remplace celui du préréglage).
    pub accent: Option<Rgb>,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        ThemeConfig {
            preset: "prism".into(),
            accent: None,
        }
    }
}

const fn hex(v: u32) -> Rgb {
    [(v >> 16) as u8, (v >> 8) as u8, v as u8]
}

/// Mélange `a` et `b` (t = 0 : a, t = 1 : b).
pub fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round().clamp(0.0, 255.0) as u8;
    [m(a[0], b[0]), m(a[1], b[1]), m(a[2], b[2])]
}

/// Luminance relative (WCAG 2.x).
pub fn luminance(c: Rgb) -> f32 {
    let lin = |v: u8| {
        let s = v as f32 / 255.0;
        if s <= 0.039_28 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
}

/// Contraste WCAG entre deux couleurs (1 à 21).
pub fn contrast(a: Rgb, b: Rgb) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// Un préréglage : identifiant, nom affiché, palette.
pub struct Preset {
    pub id: &'static str,
    pub label: &'static str,
    /// Couleurs de base ; `accent_dim` est recalculé.
    pub palette: Palette,
}

/// Palette dans l'ordre des champs de `Palette` (lisible en tableau).
#[allow(clippy::too_many_arguments)]
const fn pal(
    bg: u32,
    panel: u32,
    card: u32,
    card_hi: u32,
    border: u32,
    text: u32,
    muted: u32,
    accent: u32,
    ok: u32,
    warn: u32,
    bad: u32,
) -> Palette {
    Palette {
        bg: hex(bg),
        panel: hex(panel),
        card: hex(card),
        card_hi: hex(card_hi),
        border: hex(border),
        text: hex(text),
        muted: hex(muted),
        accent: hex(accent),
        accent_dim: hex(accent),
        ok: hex(ok),
        warn: hex(warn),
        bad: hex(bad),
    }
}

/// Préréglages, palettes publiées de projets libres (Nord, Dracula, Catppuccin,
/// Gruvbox, Tokyo Night, Solarized) adaptées aux rôles de Prism.
pub const PRESETS: [Preset; 8] = [
    Preset {
        id: "prism",
        label: "Prism",
        palette: pal(
            0x0d1117, 0x151b23, 0x1b222c, 0x222b37, 0x2a3340, 0xe6edf3, 0x8b96a3, 0x5ccfe6, 0x57d9a3, 0xe8b34b,
            0xf47067,
        ),
    },
    Preset {
        id: "nord",
        label: "Nord",
        palette: pal(
            0x2e3440, 0x3b4252, 0x3b4252, 0x434c5e, 0x4c566a, 0xeceff4, 0xb4bcc9, 0x88c0d0, 0xa3be8c, 0xebcb8b,
            0xbf616a,
        ),
    },
    Preset {
        id: "dracula",
        label: "Dracula",
        palette: pal(
            0x282a36, 0x21222c, 0x343746, 0x44475a, 0x4d5066, 0xf8f8f2, 0xb2b6c7, 0xbd93f9, 0x50fa7b, 0xf1fa8c,
            0xff5555,
        ),
    },
    Preset {
        id: "catppuccin",
        label: "Catppuccin Mocha",
        palette: pal(
            0x1e1e2e, 0x181825, 0x313244, 0x3f4053, 0x585b70, 0xcdd6f4, 0xa6adc8, 0xcba6f7, 0xa6e3a1, 0xf9e2af,
            0xf38ba8,
        ),
    },
    Preset {
        id: "gruvbox",
        label: "Gruvbox",
        palette: pal(
            0x282828, 0x1d2021, 0x3c3836, 0x494340, 0x665c54, 0xebdbb2, 0xbeaf94, 0xfe8019, 0xb8bb26, 0xfabd2f,
            0xfb4934,
        ),
    },
    Preset {
        id: "tokyonight",
        label: "Tokyo Night",
        palette: pal(
            0x1a1b26, 0x16161e, 0x24283b, 0x2f334d, 0x3b4261, 0xc0caf5, 0x9aa5ce, 0x7aa2f7, 0x9ece6a, 0xe0af68,
            0xf7768e,
        ),
    },
    Preset {
        id: "rouge",
        label: "Rouge gaming",
        palette: pal(
            0x0f0f12, 0x16161b, 0x1e1e25, 0x272730, 0x34343f, 0xeeeef2, 0x9a9aa8, 0xff3b4e, 0x4cd98a, 0xffb02e,
            0xff5a5a,
        ),
    },
    Preset {
        id: "clair",
        label: "Clair (Solarized)",
        palette: pal(
            0xfdf6e3, 0xeee8d5, 0xf5efdc, 0xe8e1cc, 0xd3cbb4, 0x253238, 0x57666c, 0x1f6f9e, 0x4d7a00, 0x8a5d00,
            0xb8261f,
        ),
    },
];

impl ThemeConfig {
    /// Palette effective : préréglage (Prism si inconnu), accent personnalisé,
    /// accent atténué recalculé.
    pub fn palette(&self) -> Palette {
        let base = PRESETS
            .iter()
            .find(|p| p.id == self.preset)
            .unwrap_or(&PRESETS[0])
            .palette;
        let mut p = base;
        if let Some(a) = self.accent {
            p.accent = a;
        }
        p.accent_dim = mix(p.bg, p.accent, 0.28);
        p
    }

    /// Le texte posé sur l'accent (boutons pleins) doit rester lisible.
    pub fn on_accent(&self) -> Rgb {
        let p = self.palette();
        if contrast(p.accent, p.bg) >= contrast(p.accent, p.text) {
            p.bg
        } else {
            p.text
        }
    }

    pub fn label(&self) -> &'static str {
        PRESETS
            .iter()
            .find(|p| p.id == self.preset)
            .unwrap_or(&PRESETS[0])
            .label
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_is_readable() {
        for p in &PRESETS {
            let t = ThemeConfig {
                preset: p.id.into(),
                accent: None,
            };
            let c = t.palette();
            for (name, bg) in [("bg", c.bg), ("card", c.card), ("card_hi", c.card_hi)] {
                assert!(
                    contrast(c.text, bg) >= 7.0,
                    "{} : texte sur {name} {:.1}",
                    p.id,
                    contrast(c.text, bg)
                );
                assert!(
                    contrast(c.muted, bg) >= 4.5,
                    "{} : texte secondaire sur {name} {:.1}",
                    p.id,
                    contrast(c.muted, bg)
                );
            }
            assert!(
                contrast(c.accent, c.bg) >= 3.0,
                "{} : accent {:.1}",
                p.id,
                contrast(c.accent, c.bg)
            );
            assert!(contrast(t.on_accent(), c.accent) >= 4.5, "{} : texte sur accent", p.id);
            assert!(contrast(c.text, c.accent_dim) >= 4.5, "{} : texte sur sélection", p.id);
        }
    }

    #[test]
    fn custom_accent_overrides_and_unknown_preset_falls_back() {
        let t = ThemeConfig {
            preset: "nord".into(),
            accent: Some([255, 0, 128]),
        };
        assert_eq!(t.palette().accent, [255, 0, 128]);
        assert_ne!(t.palette().accent_dim, t.palette().bg);
        let unknown = ThemeConfig {
            preset: "inexistant".into(),
            accent: None,
        };
        assert_eq!(unknown.palette().bg, PRESETS[0].palette.bg);
        let old: ThemeConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(old, ThemeConfig::default());
    }

    #[test]
    fn contrast_matches_wcag_reference_values() {
        assert!((contrast([0, 0, 0], [255, 255, 255]) - 21.0).abs() < 0.01);
        assert!((contrast([118, 118, 118], [255, 255, 255]) - 4.54).abs() < 0.05);
    }
}
