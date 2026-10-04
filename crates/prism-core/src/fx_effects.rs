//! Effets supplémentaires (fondu, écrasement, bascule 3D, éclatement) et animation
//! d'un changement de taille (agrandir, ancrer sur un bord, restaurer la taille).
//!
//! Même convention que `fx` : t = 0 fenêtre intacte, t = 1 fenêtre disparue ; une
//! apparition joue l'effet à l'envers.

use serde::{Deserialize, Serialize};

use crate::bar::Rect;
use crate::fx::{ease, Animation, Curve, Image};

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[inline]
fn smoothstep(a: f32, b: f32, t: f32) -> f32 {
    let x = ((t - a) / (b - a)).clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Multiplie l'alpha (et les couleurs, prémultipliées) d'un pixel.
#[inline]
pub(crate) fn fade(px: u32, a: f32) -> u32 {
    if a >= 0.999 {
        return px;
    }
    let k = (a.clamp(0.0, 1.0) * 256.0) as u32;
    let ch = |shift: u32| ((((px >> shift) & 0xff) * k) >> 8) << shift;
    ch(24) | ch(16) | ch(8) | ch(0)
}

/// Dessine `src` étiré dans le rectangle (x, y, w, h) de `out`, au plus proche.
pub(crate) fn blit_scaled(src: &Image, x: f32, y: f32, w: f32, h: f32, alpha: f32, out: &mut Image) {
    if w < 0.5 || h < 0.5 || alpha <= 0.0 || src.width == 0 || src.height == 0 {
        return;
    }
    let x0 = x.round().max(0.0) as i32;
    let y0 = y.round().max(0.0) as i32;
    let x1 = ((x + w).round() as i32).min(out.width as i32);
    let y1 = ((y + h).round() as i32).min(out.height as i32);
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let (sx, sy) = (src.width as f32 / w, src.height as f32 / h);
    // Colonne source de chaque colonne de sortie, calculée une fois.
    let cols: Vec<u32> = (x0..x1)
        .map(|ox| (((ox as f32 + 0.5 - x) * sx) as u32).min(src.width - 1))
        .collect();
    for oy in y0..y1 {
        let sy_row = (((oy as f32 + 0.5 - y) * sy) as u32).min(src.height - 1);
        let srow = (sy_row * src.width) as usize;
        let orow = (oy as u32 * out.width) as usize;
        for (i, ox) in (x0..x1).enumerate() {
            let px = src.pixels[srow + cols[i] as usize];
            if px >> 24 != 0 {
                out.pixels[orow + ox as usize] = fade(px, alpha);
            }
        }
    }
}

/// Fondu simple.
pub(crate) fn fade_out(a: &Animation, t: f32, out: &mut Image) {
    let w = a.window;
    let alpha = 1.0 - ease(Curve::EaseInOut, t);
    blit_scaled(
        &a.source,
        w.left as f32,
        w.top as f32,
        w.width() as f32,
        w.height() as f32,
        alpha,
        out,
    );
}

/// Écrasement (KDE « Squash ») : la fenêtre se tasse vers son bouton de la barre.
pub(crate) fn squash(a: &Animation, t: f32, out: &mut Image) {
    let p = ease(Curve::EaseInOut, t);
    let (w, g) = (a.window, a.target);
    let x = lerp(w.left as f32, g.left as f32, p);
    let y = lerp(w.top as f32, g.top as f32, p);
    let r = lerp(w.right as f32, g.right as f32, p);
    let b = lerp(w.bottom as f32, g.bottom as f32, p);
    let alpha = 1.0 - smoothstep(0.65, 1.0, t);
    if t >= 1.0 {
        return;
    }
    blit_scaled(&a.source, x, y, r - x, b - y, alpha, out);
}

/// Bascule (KDE « Glide ») : la fenêtre pivote vers l'arrière autour de son bord
/// inférieur, en perspective, et s'efface.
pub(crate) fn glide(a: &Animation, t: f32, out: &mut Image) {
    let src = &a.source;
    let w = a.window;
    let p = ease(Curve::EaseInOut, t);
    // ≈ 60° à 70 % d'intensité : on voit la fenêtre basculer avant qu'elle s'efface.
    let angle = p * (0.35 + 0.45 * a.intensity) * std::f32::consts::FRAC_PI_2;
    let alpha = 1.0 - smoothstep(0.3, 1.0, t);
    if alpha <= 0.0 {
        return;
    }
    let (sin, cos) = angle.sin_cos();
    let h = src.height as f32;
    let focal = 1.6 * src.width.max(src.height) as f32;
    let cx = w.left as f32 + w.width() as f32 / 2.0;
    let bottom = w.bottom as f32;
    // Projection de chaque ligne source : profondeur croissante vers le haut.
    let project = |v: f32| {
        let up = h - v; // distance au bord inférieur, dans le plan de la fenêtre
        let z = up * sin;
        let s = focal / (focal + z);
        (bottom - up * cos * s, s)
    };
    let (top_y, _) = project(0.0);
    let y0 = top_y.floor().max(0.0) as i32;
    let y1 = (bottom.ceil() as i32).min(out.height as i32);
    for oy in y0..y1 {
        // Ligne source dont la projection tombe sur `oy` (recherche par dichotomie :
        // la projection est monotone).
        let target = oy as f32 + 0.5;
        let (mut lo, mut hi) = (0.0f32, h);
        for _ in 0..14 {
            let mid = (lo + hi) / 2.0;
            if project(mid).0 < target {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let v = lo.min(h - 1.0);
        let (_, s) = project(v);
        let half = src.width as f32 * s / 2.0;
        let srow = (v as u32 * src.width) as usize;
        let x0 = (cx - half).round().max(0.0) as i32;
        let x1 = ((cx + half).round() as i32).min(out.width as i32);
        let orow = (oy as u32 * out.width) as usize;
        for ox in x0..x1 {
            let u = (((ox as f32 + 0.5 - (cx - half)) / s) as u32).min(src.width - 1);
            let px = src.pixels[srow + u as usize];
            if px >> 24 != 0 {
                out.pixels[orow + ox as usize] = fade(px, alpha);
            }
        }
    }
}

/// Bruit déterministe dans [0, 1) (pas de dépendance, même rendu à chaque fois).
fn hash(a: u32, b: u32) -> f32 {
    let mut x = a.wrapping_mul(0x9E37_79B1) ^ b.wrapping_mul(0x85EB_CA77);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x & 0xffff) as f32 / 65536.0
}

/// Éclatement (KDE « Fall Apart ») : la fenêtre se brise en carreaux qui s'écartent,
/// tournent et tombent en s'effaçant.
pub(crate) fn fall_apart(a: &Animation, t: f32, out: &mut Image) {
    let src = &a.source;
    let w = a.window;
    if src.width == 0 || src.height == 0 || t >= 1.0 {
        return;
    }
    let block = (src.width.max(src.height) / 14).clamp(24, 64);
    // Se fissure doucement puis accélère.
    let p = t.powf(1.5);
    let alpha = 1.0 - smoothstep(0.35, 1.0, t);
    let (wcx, wcy) = (src.width as f32 / 2.0, src.height as f32 / 2.0);
    let spread = 0.35 * (0.6 + a.intensity) * src.width.max(src.height) as f32;
    let mut by = 0;
    while by < src.height {
        let mut bx = 0;
        while bx < src.width {
            let (bw, bh) = (block.min(src.width - bx), block.min(src.height - by));
            let (r1, r2, r3) = (hash(bx, by), hash(by + 7, bx + 3), hash(bx ^ by, 11));
            // Centre du carreau, direction depuis le centre de la fenêtre.
            let (ccx, ccy) = (bx as f32 + bw as f32 / 2.0, by as f32 + bh as f32 / 2.0);
            let (dx, dy) = (ccx - wcx, ccy - wcy);
            let len = (dx * dx + dy * dy).sqrt().max(1.0);
            let speed = spread * (0.5 + r1);
            let ox = dx / len * speed * p + (r2 - 0.5) * 60.0 * p;
            let oy = dy / len * speed * p + 0.6 * spread * p * p; // un peu de chute
            let angle = (r3 - 0.5) * 2.4 * p;
            let scale = 1.0 - 0.35 * p;
            let (sin, cos) = angle.sin_cos();
            let (px, py) = (w.left as f32 + ccx + ox, w.top as f32 + ccy + oy);
            // Boîte englobante du carreau tourné, puis échantillonnage inverse.
            let ext = (bw.max(bh) as f32) * scale * 0.75 + 1.0;
            let (x0, x1) = ((px - ext).floor() as i32, (px + ext).ceil() as i32);
            let (y0, y1) = ((py - ext).floor() as i32, (py + ext).ceil() as i32);
            for yy in y0.max(0)..y1.min(out.height as i32) {
                for xx in x0.max(0)..x1.min(out.width as i32) {
                    let (rx, ry) = (xx as f32 + 0.5 - px, yy as f32 + 0.5 - py);
                    // Rotation inverse puis mise à l'échelle inverse.
                    let lx = (rx * cos + ry * sin) / scale + bw as f32 / 2.0;
                    let ly = (-rx * sin + ry * cos) / scale + bh as f32 / 2.0;
                    if lx < 0.0 || ly < 0.0 || lx >= bw as f32 || ly >= bh as f32 {
                        continue;
                    }
                    let pxl = src.pixels[((by + ly as u32) * src.width + bx + lx as u32) as usize];
                    if pxl >> 24 != 0 {
                        out.pixels[(yy as u32 * out.width + xx as u32) as usize] = fade(pxl, alpha);
                    }
                }
            }
            bx += block;
        }
        by += block;
    }
}

// --- Changement de taille --------------------------------------------------------

/// Animation d'un agrandissement, d'un ancrage sur un bord ou d'un retour à la taille
/// normale.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MorphEffect {
    None,
    /// La fenêtre glisse et s'étire de l'ancien cadre au nouveau.
    Glide,
    /// Même trajet, avec un rebond élastique à l'arrivée.
    Jelly,
    /// Fondu enchaîné sur place.
    Fade,
}

impl MorphEffect {
    pub const ALL: [MorphEffect; 4] = [
        MorphEffect::None,
        MorphEffect::Glide,
        MorphEffect::Jelly,
        MorphEffect::Fade,
    ];

    pub fn label(self) -> &'static str {
        match self {
            MorphEffect::None => "Aucun",
            MorphEffect::Glide => "Glisse",
            MorphEffect::Jelly => "Gélatine",
            MorphEffect::Fade => "Fondu enchaîné",
        }
    }
}

/// La copie de la fenêtre passe de `from` à `to` (repère de la couche d'affichage),
/// pendant que la vraie fenêtre, déjà à sa nouvelle taille, apparaît dessous.
#[derive(Clone, Debug)]
pub struct Morph {
    pub effect: MorphEffect,
    pub source: Image,
    pub from: Rect,
    pub to: Rect,
    pub intensity: f32,
}

impl Morph {
    /// Cadre de la copie au temps t.
    pub fn frame(&self, t: f32) -> (f32, f32, f32, f32) {
        let (f, g) = (self.from, self.to);
        let p = match self.effect {
            MorphEffect::Glide => ease(Curve::EaseInOut, t),
            MorphEffect::Jelly => {
                // Ressort : dépasse le cadre final d'autant plus que l'intensité est forte.
                let (s, e) = (ease(Curve::Spring, t), ease(Curve::EaseOut, t));
                e + (s - e) * (0.4 + 0.8 * self.intensity)
            }
            MorphEffect::Fade | MorphEffect::None => 0.0,
        };
        let x = lerp(f.left as f32, g.left as f32, p);
        let y = lerp(f.top as f32, g.top as f32, p);
        let r = lerp(f.right as f32, g.right as f32, p);
        let b = lerp(f.bottom as f32, g.bottom as f32, p);
        let (mut w, mut h) = (r - x, b - y);
        let (mut x, mut y) = (x, y);
        if self.effect == MorphEffect::Jelly {
            // Écrasement/étirement amorti autour du centre.
            let wob = (t * 18.0).sin() * (-4.5 * t).exp() * 0.06 * self.intensity;
            let (cx, cy) = (x + w / 2.0, y + h / 2.0);
            w *= 1.0 + wob;
            h *= 1.0 - wob;
            x = cx - w / 2.0;
            y = cy - h / 2.0;
        }
        (x, y, w, h)
    }

    /// Opacité de la copie au temps t.
    pub fn copy_alpha(&self, t: f32) -> f32 {
        match self.effect {
            MorphEffect::Fade => 1.0 - smoothstep(0.0, 1.0, t),
            MorphEffect::Jelly => 1.0 - smoothstep(0.7, 1.0, t),
            _ => 1.0 - smoothstep(0.55, 1.0, t),
        }
    }

    /// Opacité de la vraie fenêtre (sous la copie) au temps t.
    pub fn window_alpha(&self, t: f32) -> f32 {
        match self.effect {
            MorphEffect::Fade => smoothstep(0.0, 1.0, t),
            MorphEffect::Jelly => smoothstep(0.6, 0.95, t),
            _ => smoothstep(0.45, 0.95, t),
        }
    }

    /// Dessine la copie au temps t dans `out` (effacée d'abord).
    pub fn render(&self, t: f32, out: &mut Image) {
        out.clear();
        let t = t.clamp(0.0, 1.0);
        let (x, y, w, h) = self.frame(t);
        blit_scaled(&self.source, x, y, w, h, self.copy_alpha(t), out);
    }

    /// Zone à couvrir par la couche d'affichage : tout le trajet, rebond compris.
    pub fn bounds(&self) -> Rect {
        let (mut l, mut t, mut r, mut b) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for i in 0..=120 {
            let (x, y, w, h) = self.frame(i as f32 / 120.0);
            l = l.min(x);
            t = t.min(y);
            r = r.max(x + w);
            b = b.max(y + h);
        }
        Rect {
            left: l.floor() as i32 - 2,
            top: t.floor() as i32 - 2,
            right: r.ceil() as i32 + 2,
            bottom: b.ceil() as i32 + 2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::sample_window;

    fn morph(effect: MorphEffect) -> Morph {
        Morph {
            effect,
            source: sample_window(300, 200),
            from: Rect {
                left: 100,
                top: 100,
                right: 400,
                bottom: 300,
            },
            to: Rect {
                left: 0,
                top: 0,
                right: 800,
                bottom: 560,
            },
            intensity: 0.7,
        }
    }

    #[test]
    fn morph_starts_on_the_old_frame_and_hands_over_to_the_window() {
        for e in [MorphEffect::Glide, MorphEffect::Jelly] {
            let m = morph(e);
            let mut out = Image::new(800, 600);
            m.render(0.0, &mut out);
            assert_eq!(out.bounds(), Some(m.from), "{e:?}");
            let (x, y, w, h) = m.frame(1.0);
            assert!((x - 0.0).abs() < 2.0 && (y - 0.0).abs() < 2.0, "{e:?}");
            assert!((w - 800.0).abs() < 4.0 && (h - 560.0).abs() < 4.0, "{e:?}");
            assert!(m.copy_alpha(1.0) < 0.01 && m.window_alpha(1.0) > 0.99, "{e:?}");
            assert!(m.window_alpha(0.0) < 0.01, "{e:?}");
        }
    }

    #[test]
    fn jelly_morph_overshoots_the_target_inside_its_bounds() {
        let m = morph(MorphEffect::Jelly);
        let widest = (0..100).map(|i| m.frame(i as f32 / 100.0).2).fold(0.0f32, f32::max);
        assert!(widest > 800.0 + 4.0, "rebond : {widest}");
        let b = m.bounds();
        for i in 0..100 {
            let (x, y, w, h) = m.frame(i as f32 / 100.0);
            assert!(x >= b.left as f32 - 1.0 && y >= b.top as f32 - 1.0, "t={i}");
            assert!(x + w <= b.right as f32 + 1.0 && y + h <= b.bottom as f32 + 1.0, "t={i}");
        }
    }

    #[test]
    fn fade_morph_stays_in_place() {
        let m = morph(MorphEffect::Fade);
        for i in 0..=10 {
            let (x, y, w, h) = m.frame(i as f32 / 10.0);
            assert_eq!((x, y, w, h), (100.0, 100.0, 300.0, 200.0));
        }
    }

    #[test]
    fn scaled_blit_is_exact_at_scale_one() {
        let src = sample_window(50, 30);
        let mut out = Image::new(100, 100);
        blit_scaled(&src, 10.0, 20.0, 50.0, 30.0, 1.0, &mut out);
        for y in 0..30 {
            for x in 0..50 {
                assert_eq!(
                    out.pixels[((y + 20) * 100 + x + 10) as usize],
                    src.pixels[(y * 50 + x) as usize]
                );
            }
        }
    }
}
