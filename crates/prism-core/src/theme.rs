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
    /// Couleurs choisies par l'utilisateur, par rôle (`ROLES`) : remplacent celles du
    /// préréglage (fond, cartes, texte…).
    pub colors: std::collections::BTreeMap<String, Rgb>,
}

/// Rôles de couleur personnalisables : identifiant, nom affiché.
pub const ROLES: [(&str, &str); 11] = [
    ("bg", "Fond"),
    ("panel", "Panneaux"),
    ("card", "Cartes"),
    ("card_hi", "Cartes en relief"),
    ("border", "Bordures"),
    ("text", "Texte"),
    ("muted", "Texte secondaire"),
    ("accent", "Accent"),
    ("ok", "Succès"),
    ("warn", "Avertissement"),
    ("bad", "Erreur"),
];

impl Palette {
    /// Couleur d'un rôle (`ROLES`).
    pub fn get(&self, role: &str) -> Option<Rgb> {
        Some(match role {
            "bg" => self.bg,
            "panel" => self.panel,
            "card" => self.card,
            "card_hi" => self.card_hi,
            "border" => self.border,
            "text" => self.text,
            "muted" => self.muted,
            "accent" => self.accent,
            "ok" => self.ok,
            "warn" => self.warn,
            "bad" => self.bad,
            _ => return None,
        })
    }

    fn set(&mut self, role: &str, c: Rgb) {
        match role {
            "bg" => self.bg = c,
            "panel" => self.panel = c,
            "card" => self.card = c,
            "card_hi" => self.card_hi = c,
            "border" => self.border = c,
            "text" => self.text = c,
            "muted" => self.muted = c,
            "accent" => self.accent = c,
            "ok" => self.ok = c,
            "warn" => self.warn = c,
            "bad" => self.bad = c,
            _ => {}
        }
    }
}

impl Default for ThemeConfig {
    fn default() -> Self {
        ThemeConfig {
            preset: "prism".into(),
            accent: None,
            colors: Default::default(),
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
pub const PRESETS: [Preset; 14] = [
    // Prism : bleu nuit profond et cyan lumineux (futuriste, sobre).
    Preset {
        id: "prism",
        label: "Prism",
        palette: pal(
            0x070b14, 0x0b1120, 0x101828, 0x16213a, 0x1f2d4a, 0xe6f1ff, 0x93a4c3, 0x22d3ee, 0x34d399, 0xfbbf24,
            0xf87171,
        ),
    },
    Preset {
        id: "neon",
        label: "Néon",
        palette: pal(
            0x05050a, 0x0a0a14, 0x10101e, 0x18182b, 0x26263f, 0xf5f3ff, 0xa3a0c2, 0xff2bd6, 0x2bffb1, 0xffe14d,
            0xff4d6d,
        ),
    },
    Preset {
        id: "cyberpunk",
        label: "Cyberpunk",
        palette: pal(
            0x0b0b10, 0x111118, 0x171722, 0x20202e, 0x2c2c3e, 0xfff9d6, 0xb9b39a, 0xfcee0a, 0x00f0ff, 0xff9f1c,
            0xff3864,
        ),
    },
    Preset {
        id: "holo",
        label: "Holo",
        palette: pal(
            0x0a0f1f, 0x0f1630, 0x141d3d, 0x1b2752, 0x29386b, 0xeef2ff, 0x9aa6d1, 0xa78bfa, 0x5eead4, 0xfde68a,
            0xfda4af,
        ),
    },
    Preset {
        id: "synthwave",
        label: "Synthwave",
        palette: pal(
            0x140a24, 0x1b0f30, 0x22143c, 0x2c1a4d, 0x3d2766, 0xfdf0ff, 0xc2a8d9, 0xff6ac1, 0x72f1b8, 0xfede5d,
            0xfe4450,
        ),
    },
    Preset {
        id: "aurora",
        label: "Aurora",
        palette: pal(
            0x061417, 0x0a1d21, 0x0e262b, 0x143339, 0x1e4750, 0xe8fffb, 0x8fbdb5, 0x2dd4bf, 0x86efac, 0xfcd34d,
            0xfb7185,
        ),
    },
    Preset {
        id: "carbone",
        label: "Carbone",
        palette: pal(
            0x0a0a0a, 0x111111, 0x171717, 0x1f1f1f, 0x2a2a2a, 0xfafafa, 0xa3a3a3, 0xe5e5e5, 0x4ade80, 0xfacc15,
            0xf87171,
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
        for (role, c) in &self.colors {
            p.set(role, *c);
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
                colors: Default::default(),
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
            colors: Default::default(),
            accent: Some([255, 0, 128]),
        };
        assert_eq!(t.palette().accent, [255, 0, 128]);
        assert_ne!(t.palette().accent_dim, t.palette().bg);
        let unknown = ThemeConfig {
            preset: "inexistant".into(),
            colors: Default::default(),
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
