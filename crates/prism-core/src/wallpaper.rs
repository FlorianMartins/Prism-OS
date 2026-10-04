//! Fond d'écran futuriste aux couleurs du thème : dégradé profond, horizon lumineux,
//! grille en perspective qui s'éloigne, grand losange en filigrane. Calculé ici (pur,
//! testé), écrit en BMP 24 bits par la plateforme puis posé comme fond d'écran.

use crate::theme::{mix, Palette, Rgb};

fn blend(base: Rgb, over: Rgb, a: f32) -> Rgb {
    mix(base, over, a.clamp(0.0, 1.0))
}

/// Image RGB (`w × h × 3` octets, ligne par ligne, du haut vers le bas).
pub fn render(w: usize, h: usize, p: &Palette) -> Vec<u8> {
    let mut px = vec![0u8; w * h * 3];
    let horizon = h as f32 * 0.62;
    let cx = w as f32 / 2.0;
    let accent2 = mix(p.accent, [167, 139, 250], 0.5);
    for y in 0..h {
        for x in 0..w {
            let fy = y as f32;
            let fx = x as f32;
            // Ciel : du fond vers les panneaux, vignette sur les bords.
            let t = fy / h as f32;
            let mut c = mix(p.bg, p.panel, (1.0 - (t - 0.55).abs() * 2.0).clamp(0.0, 1.0) * 0.8);
            let dx = (fx - cx) / w as f32;
            let vignette = (dx * dx * 1.6 + (t - 0.5) * (t - 0.5) * 1.2).min(1.0);
            c = blend(c, [0, 0, 0], vignette * 0.55);
            // Halo de l'horizon.
            let dh = (fy - horizon).abs() / h as f32;
            c = blend(c, p.accent, (0.10 - dh).max(0.0) * 4.5);
            c = blend(c, accent2, (0.035 - dh).max(0.0) * 9.0);
            // Sol : grille en perspective (lignes de fuite + lignes horizontales).
            if fy > horizon {
                let depth = (fy - horizon) / (h as f32 - horizon); // 0 à l'horizon, 1 en bas
                let z = 1.0 / depth.max(0.02);
                let fade = depth.powf(0.8);
                // Lignes horizontales, plus serrées vers l'horizon.
                let gz = (z * 2.2).fract();
                if gz < 0.06 * (1.0 + depth) {
                    c = blend(c, p.accent, 0.55 * fade);
                }
                // Lignes de fuite.
                let gx = ((fx - cx) / (w as f32) * z * 9.0).fract().abs();
                if gx < 0.035 * depth.max(0.25) || gx > 1.0 - 0.035 * depth.max(0.25) {
                    c = blend(c, accent2, 0.5 * fade);
                }
            }
            // Ligne d'horizon nette.
            if (fy - horizon).abs() < 1.5 {
                c = blend(c, [255, 255, 255], 0.5);
            }
            px[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&c);
        }
    }
    // Grand losange en filigrane au-dessus de l'horizon (arêtes fines).
    let s = h as f32 * 0.22;
    let top = (cx, horizon - s * 2.05);
    let mid_l = (cx - s * 0.62, horizon - s * 1.05);
    let mid_r = (cx + s * 0.62, horizon - s * 1.05);
    let bot = (cx, horizon - s * 0.05);
    for (a, b, col) in [
        (top, mid_l, p.accent),
        (top, mid_r, p.accent),
        (mid_l, bot, accent2),
        (mid_r, bot, accent2),
        (mid_l, mid_r, mix(p.accent, accent2, 0.5)),
        (top, bot, mix(p.accent, accent2, 0.5)),
    ] {
        line(&mut px, w, h, a, b, col, if a == top && b == bot { 0.18 } else { 0.7 });
    }
    px
}

fn line(px: &mut [u8], w: usize, h: usize, a: (f32, f32), b: (f32, f32), col: Rgb, alpha: f32) {
    let n = ((b.0 - a.0).abs().max((b.1 - a.1).abs()) * 2.0) as usize + 1;
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let (x, y) = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
        for (ox, oy, k) in [
            (0i32, 0i32, 1.0f32),
            (1, 0, 0.35),
            (-1, 0, 0.35),
            (0, 1, 0.35),
            (0, -1, 0.35),
        ] {
            let (xi, yi) = (x as i32 + ox, y as i32 + oy);
            if xi >= 0 && yi >= 0 && (xi as usize) < w && (yi as usize) < h {
                let o = (yi as usize * w + xi as usize) * 3;
                let cur = [px[o], px[o + 1], px[o + 2]];
                px[o..o + 3].copy_from_slice(&blend(cur, col, alpha * k));
            }
        }
    }
}

/// BMP 24 bits (lignes du bas vers le haut, alignées sur 4 octets).
pub fn bmp(w: usize, h: usize, rgb: &[u8]) -> Vec<u8> {
    let row = (w * 3).div_ceil(4) * 4;
    let size = 54 + row * h;
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&(size as u32).to_le_bytes());
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(h as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&24u16.to_le_bytes());
    out.extend_from_slice(&[0; 24]);
    for y in (0..h).rev() {
        for x in 0..w {
            let o = (y * w + x) * 3;
            out.extend_from_slice(&[rgb[o + 2], rgb[o + 1], rgb[o]]);
        }
        out.resize(out.len() + row - w * 3, 0);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeConfig;

    #[test]
    fn wallpaper_uses_the_theme_and_bmp_is_well_formed() {
        let p = ThemeConfig::default().palette();
        let img = render(320, 180, &p);
        assert_eq!(img.len(), 320 * 180 * 3);
        // Le coin haut gauche est sombre, l'horizon est lumineux.
        let at = |x: usize, y: usize| {
            let o = (y * 320 + x) * 3;
            img[o] as u32 + img[o + 1] as u32 + img[o + 2] as u32
        };
        assert!(at(2, 2) < 60, "coin {}", at(2, 2));
        assert!(at(160, (180.0 * 0.62) as usize) > 300, "horizon {}", at(160, 111));
        let b = bmp(320, 180, &img);
        assert_eq!(&b[..2], b"BM");
        assert_eq!(b.len(), 54 + 320 * 3 * 180);
        assert_eq!(u32::from_le_bytes([b[2], b[3], b[4], b[5]]) as usize, b.len());
    }
}

#[cfg(test)]
mod preview {
    /// Aperçu (outil) : `cargo test -p prism-core wallpaper_preview -- --ignored`.
    #[test]
    #[ignore]
    fn wallpaper_preview() {
        for id in ["prism", "neon", "synthwave"] {
            let t = crate::theme::ThemeConfig {
                preset: id.into(),
                ..Default::default()
            };
            let img = super::render(960, 540, &t.palette());
            let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-shots");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(format!("fond-{id}.bmp")), super::bmp(960, 540, &img)).unwrap();
        }
    }
}
