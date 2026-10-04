//! Couleur d'accent de Windows calée sur le thème de Prism : valeurs au format attendu
//! par Windows (registre `DWM` et `Explorer\Accent`), calculées ici et testées.

use crate::theme::{mix, Rgb};

/// `AccentColor`, `AccentColorMenu`, `StartColorMenu` : DWORD 0xAABBGGRR.
pub fn abgr(c: Rgb) -> u32 {
    0xFF00_0000 | (c[2] as u32) << 16 | (c[1] as u32) << 8 | c[0] as u32
}

/// `ColorizationColor`, `ColorizationAfterglow` : DWORD 0xAARRGGBB.
pub fn argb(c: Rgb) -> u32 {
    0xC400_0000 | (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32
}

/// Les 8 nuances de `AccentPalette` (RGBA, 32 octets) : trois plus claires, l'accent,
/// quatre plus sombres — comme Windows les calcule pour un accent choisi.
pub fn accent_palette(c: Rgb) -> [u8; 32] {
    let shades = [
        mix(c, [255, 255, 255], 0.60),
        mix(c, [255, 255, 255], 0.40),
        mix(c, [255, 255, 255], 0.20),
        c,
        mix(c, [0, 0, 0], 0.20),
        mix(c, [0, 0, 0], 0.40),
        mix(c, [0, 0, 0], 0.60),
        mix(c, [0, 0, 0], 0.75),
    ];
    let mut out = [0u8; 32];
    for (i, s) in shades.iter().enumerate() {
        out[i * 4..i * 4 + 3].copy_from_slice(s);
    }
    out
}

/// Nuance utilisée par le menu Démarrer (la première plus sombre).
pub fn start_color(c: Rgb) -> Rgb {
    mix(c, [0, 0, 0], 0.20)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_color_formats() {
        let cyan = [0x22, 0xd3, 0xee];
        assert_eq!(abgr(cyan), 0xFFEED322);
        assert_eq!(argb(cyan), 0xC422D3EE);
        let p = accent_palette(cyan);
        assert_eq!(&p[12..15], &cyan, "4e nuance = l'accent");
        assert!(p[0] > cyan[0] && p[28] < cyan[0], "claires puis sombres");
        assert!(p.iter().skip(3).step_by(4).all(|a| *a == 0), "alpha à 0");
    }
}

/// Valeurs d'origine (avant Prism), pour tout remettre.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Origine {
    /// `None` : l'accent n'a pas été changé par Prism ; `Some` : les valeurs d'avant
    /// (chacune `None` si elle n'existait pas).
    pub accent: Option<AccentAvant>,
    /// Fond d'écran d'avant (chemin), s'il a été changé.
    pub fond: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AccentAvant {
    pub accent_color: Option<u32>,
    pub colorization_color: Option<u32>,
    pub colorization_afterglow: Option<u32>,
    pub accent_color_menu: Option<u32>,
    pub start_color_menu: Option<u32>,
    pub accent_palette: Option<Vec<u8>>,
}

impl Origine {
    pub const FICHIER: &'static str = "design-windows.json";

    pub fn charger(dir: &std::path::Path) -> Origine {
        std::fs::read(dir.join(Self::FICHIER))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn enregistrer(&self, dir: &std::path::Path) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let json = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(dir.join(Self::FICHIER), json).map_err(|e| e.to_string())
    }
}
