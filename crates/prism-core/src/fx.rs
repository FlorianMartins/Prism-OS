//! Moteur d'effets de Prism : génie (aspiration), gélatine, zoom/fondu.
//!
//! Rendu logiciel pur : on part d'une copie de la fenêtre (image BGRA prémultipliée,
//! le format que veut `UpdateLayeredWindow`) et on calcule chaque image de
//! l'animation. Aucune dépendance Windows : testé et prévisualisé sous Linux.

use serde::{Deserialize, Serialize};

use crate::bar::{Edge, Rect};

/// Image BGRA prémultipliée (0xAARRGGBB en mémoire little-endian = B, G, R, A).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u32>,
}

impl Image {
    pub fn new(width: u32, height: u32) -> Image {
        Image {
            width,
            height,
            pixels: vec![0; (width as usize) * (height as usize)],
        }
    }

    pub fn clear(&mut self) {
        self.pixels.iter_mut().for_each(|p| *p = 0);
    }

    #[inline]
    fn get(&self, x: u32, y: u32) -> u32 {
        self.pixels[(y * self.width + x) as usize]
    }

    #[inline]
    fn put(&mut self, x: i32, y: i32, px: u32) {
        if x >= 0 && y >= 0 && (x as u32) < self.width && (y as u32) < self.height {
            self.pixels[(y as u32 * self.width + x as u32) as usize] = px;
        }
    }

    /// Nombre de pixels non transparents (tests).
    pub fn coverage(&self) -> usize {
        self.pixels.iter().filter(|p| *p >> 24 != 0).count()
    }

    /// Boîte englobante des pixels non transparents (tests).
    pub fn bounds(&self) -> Option<Rect> {
        let mut b: Option<Rect> = None;
        for y in 0..self.height {
            for x in 0..self.width {
                if self.get(x, y) >> 24 != 0 {
                    let (x, y) = (x as i32, y as i32);
                    b = Some(match b {
                        None => Rect {
                            left: x,
                            top: y,
                            right: x + 1,
                            bottom: y + 1,
                        },
                        Some(r) => Rect {
                            left: r.left.min(x),
                            top: r.top.min(y),
                            right: r.right.max(x + 1),
                            bottom: r.bottom.max(y + 1),
                        },
                    });
                }
            }
        }
        b
    }
}

/// Multiplie l'alpha (et les couleurs, prémultipliées) d'un pixel.
#[inline]
fn fade(px: u32, a: f32) -> u32 {
    if a >= 0.999 {
        return px;
    }
    let k = (a.clamp(0.0, 1.0) * 256.0) as u32;
    let ch = |shift: u32| (((px >> shift) & 0xff) * k >> 8) << shift;
    ch(24) | ch(16) | ch(8) | ch(0)
}

// --- Courbes ----------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Curve {
    EaseOut,
    EaseInOut,
    /// Ressort amorti : dépasse un peu puis se stabilise.
    Spring,
}

pub fn ease(curve: Curve, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    match curve {
        Curve::EaseOut => 1.0 - (1.0 - t).powi(3),
        Curve::EaseInOut => {
            if t < 0.5 {
                4.0 * t * t * t
            } else {
                1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
            }
        }
        Curve::Spring => 1.0 - (-6.0 * t).exp() * (t * 14.0).cos(),
    }
}

// --- Effets -----------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    /// Rien : l'événement passe sans animation Prism.
    None,
    /// Lampe de génie : la fenêtre est aspirée vers son bouton de la barre.
    Genie,
    /// Gélatine : apparition avec écrasement, étirement et ondulation amortis.
    Jelly,
    /// Zoom et fondu.
    Zoom,
}

impl Effect {
    pub const ALL: [Effect; 4] = [Effect::None, Effect::Genie, Effect::Jelly, Effect::Zoom];

    pub fn label(self) -> &'static str {
        match self {
            Effect::None => "Aucun",
            Effect::Genie => "Lampe de génie",
            Effect::Jelly => "Gélatine",
            Effect::Zoom => "Zoom et fondu",
        }
    }
}

/// Réglages des effets (dans `bar.json`, la barre les joue).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FxConfig {
    pub enabled: bool,
    pub minimize: Effect,
    pub restore: Effect,
    pub open: Effect,
    pub close: Effect,
    /// Durée en millisecondes.
    pub duration_ms: u32,
    /// Intensité de la déformation, en pourcentage.
    pub intensity: u8,
}

impl Default for FxConfig {
    fn default() -> Self {
        FxConfig {
            enabled: false,
            minimize: Effect::Genie,
            restore: Effect::Genie,
            open: Effect::Jelly,
            close: Effect::Zoom,
            duration_ms: 320,
            intensity: 70,
        }
    }
}

pub const DURATION_MIN: u32 = 120;
pub const DURATION_MAX: u32 = 900;

impl FxConfig {
    pub fn sanitized(mut self) -> FxConfig {
        self.duration_ms = self.duration_ms.clamp(DURATION_MIN, DURATION_MAX);
        self.intensity = self.intensity.min(100);
        self
    }
}

/// Une animation à jouer : l'image de la fenêtre, d'où elle part, où elle va.
#[derive(Clone, Debug)]
pub struct Animation {
    pub effect: Effect,
    /// Copie de la fenêtre (taille de `window`).
    pub source: Image,
    /// Position de la fenêtre, dans le repère de la couche d'affichage.
    pub window: Rect,
    /// Cible du génie (le bouton de la fenêtre dans la barre).
    pub target: Rect,
    /// Bord de la barre : le génie aspire vers ce bord.
    pub edge: Edge,
    /// `true` : l'animation se joue à l'envers (restauration, fermeture -> ouverture…).
    pub reverse: bool,
    pub intensity: f32,
}

impl Animation {
    /// Dessine l'image de l'animation au temps `t` (0 = début, 1 = fin) dans `out`
    /// (repère de la couche d'affichage). `out` est effacée d'abord.
    pub fn render(&self, t: f32, out: &mut Image) {
        out.clear();
        let t = if self.reverse {
            1.0 - t.clamp(0.0, 1.0)
        } else {
            t.clamp(0.0, 1.0)
        };
        match self.effect {
            Effect::None => blit(&self.source, self.window, 1.0, out),
            Effect::Genie => genie(self, t, out),
            Effect::Jelly => jelly(self, t, out),
            Effect::Zoom => zoom(self, t, out),
        }
    }
}

fn blit(src: &Image, at: Rect, alpha: f32, out: &mut Image) {
    for y in 0..src.height {
        for x in 0..src.width {
            let px = src.get(x, y);
            if px >> 24 != 0 {
                out.put(at.left + x as i32, at.top + y as i32, fade(px, alpha));
            }
        }
    }
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Génie : chaque ligne de la fenêtre (le long de l'axe vers la barre) avance avec
/// un retard qui dépend de sa distance à la barre ; sa largeur se resserre d'abord
/// en entonnoir vers le bouton, puis elle glisse dedans.
fn genie(a: &Animation, t: f32, out: &mut Image) {
    let src = &a.source;
    let vertical = a.edge.horizontal(); // barre en haut/bas : aspiration verticale
    let (len, span) = if vertical {
        (src.height, src.width)
    } else {
        (src.width, src.height)
    };
    if len == 0 || span == 0 {
        return;
    }
    let w = a.window;
    let g = a.target;
    let k = 0.4 + 0.6 * a.intensity; // intensité de l'entonnoir
                                     // Repère : `u` le long de l'axe (lignes), `s` en travers.
    let (win_u0, win_u1, win_s0, win_s1, tgt_u, tgt_s0, tgt_s1) = if vertical {
        (
            w.top as f32,
            w.bottom as f32,
            w.left as f32,
            w.right as f32,
            g.top as f32 + g.height() as f32 / 2.0,
            g.left as f32,
            g.right as f32,
        )
    } else {
        (
            w.left as f32,
            w.right as f32,
            w.top as f32,
            w.bottom as f32,
            g.left as f32 + g.width() as f32 / 2.0,
            g.top as f32,
            g.bottom as f32,
        )
    };
    // La barre est-elle « après » la fenêtre le long de l'axe (bas, droite) ?
    let toward_end = tgt_u >= (win_u0 + win_u1) / 2.0;
    let line_pos = |i: f32| -> (f32, f32, f32) {
        // v : 0 pour la ligne la plus éloignée de la barre, 1 pour la plus proche.
        let v = if toward_end {
            i / len as f32
        } else {
            1.0 - i / len as f32
        };
        let pinch = ease(
            Curve::EaseInOut,
            ((t * 2.4 * k) - (1.0 - v) * (k * 0.9)).clamp(0.0, 1.0),
        );
        let slide = ease(Curve::EaseInOut, ((t * 1.8) - (1.0 - v) * 0.8).clamp(0.0, 1.0));
        let u = lerp(lerp(win_u0, win_u1, i / len as f32), tgt_u, slide);
        let s0 = lerp(win_s0, tgt_s0, pinch);
        let s1 = lerp(win_s1, tgt_s1, pinch);
        (u, s0, s1)
    };
    for i in 0..len {
        let (u, s0, s1) = line_pos(i as f32);
        let (u_next, _, _) = line_pos(i as f32 + 1.0);
        let (ua, ub) = if u <= u_next { (u, u_next) } else { (u_next, u) };
        let rows_from = ua.floor() as i32;
        let rows_to = (ub.ceil() as i32).max(rows_from + 1);
        let width = (s1 - s0).max(0.0);
        if width < 0.5 {
            continue;
        }
        // Fondu léger en toute fin d'aspiration.
        let alpha = 1.0 - ((t - 0.85) / 0.15).clamp(0.0, 1.0);
        let dst0 = s0.floor() as i32;
        let dst1 = s1.ceil() as i32;
        for d in dst0..dst1 {
            let f = ((d as f32 + 0.5 - s0) / width).clamp(0.0, 0.9999);
            let j = (f * span as f32) as u32;
            let px = if vertical { src.get(j, i) } else { src.get(i, j) };
            if px >> 24 == 0 {
                continue;
            }
            let px = fade(px, alpha);
            for r in rows_from..rows_to {
                if vertical {
                    out.put(d, r, px);
                } else {
                    out.put(r, d, px);
                }
            }
        }
    }
}

/// Gélatine : la fenêtre apparaît en s'écrasant et s'étirant autour de son centre,
/// avec une ondulation qui se propage et s'amortit (t = 0 : invisible, 1 : au repos).
fn jelly(a: &Animation, t: f32, out: &mut Image) {
    let src = &a.source;
    let w = a.window;
    let (cx, cy) = (
        w.left as f32 + w.width() as f32 / 2.0,
        w.top as f32 + w.height() as f32 / 2.0,
    );
    let amp = 0.26 * a.intensity;
    let decay = (-3.8 * t).exp();
    let osc = (t * 16.0).sin() * decay;
    // Ressort : la fenêtre dépasse un instant sa taille finale puis s'y pose.
    let grow = ease(Curve::Spring, t);
    let scale = lerp(0.82, 1.0, grow);
    let sx = scale * (1.0 + amp * osc);
    let sy = scale * (1.0 - amp * osc);
    let wave = 26.0 * a.intensity * decay;
    let alpha = (t / 0.3).clamp(0.0, 1.0);
    let (hw, hh) = (src.width as f32 / 2.0, src.height as f32 / 2.0);
    // Échantillonnage inverse : chaque pixel de sortie va chercher son pixel source.
    let reach_x = (hw * sx).ceil() as i32 + wave as i32 + 2;
    let reach_y = (hh * sy).ceil() as i32 + 2;
    for oy in -reach_y..reach_y {
        let ry = oy as f32;
        let ny = ry / hh.max(1.0); // -1..1
        let shift = wave * (ny * 3.2 + t * 22.0).sin() * (1.0 - ny.abs());
        for ox in -reach_x..reach_x {
            let rx = ox as f32 - shift;
            let fx = rx / sx + hw;
            let fy = ry / sy + hh;
            if fx < 0.0 || fy < 0.0 || fx >= src.width as f32 || fy >= src.height as f32 {
                continue;
            }
            let px = src.get(fx as u32, fy as u32);
            if px >> 24 != 0 {
                out.put((cx + ox as f32) as i32, (cy + oy as f32) as i32, fade(px, alpha));
            }
        }
    }
}

/// Zoom : t = 0 au repos, 1 réduite à 80 % et transparente (fermeture).
fn zoom(a: &Animation, t: f32, out: &mut Image) {
    let src = &a.source;
    let w = a.window;
    let p = ease(Curve::EaseOut, t);
    let scale = lerp(1.0, 0.8, p);
    let alpha = 1.0 - p;
    let (cx, cy) = (
        w.left as f32 + w.width() as f32 / 2.0,
        w.top as f32 + w.height() as f32 / 2.0,
    );
    let (hw, hh) = (src.width as f32 * scale / 2.0, src.height as f32 * scale / 2.0);
    for oy in (-hh as i32)..(hh as i32) {
        for ox in (-hw as i32)..(hw as i32) {
            let fx = (ox as f32 + hw) / scale;
            let fy = (oy as f32 + hh) / scale;
            if fx >= src.width as f32 || fy >= src.height as f32 || fx < 0.0 || fy < 0.0 {
                continue;
            }
            let px = src.get(fx as u32, fy as u32);
            if px >> 24 != 0 {
                out.put((cx + ox as f32) as i32, (cy + oy as f32) as i32, fade(px, alpha));
            }
        }
    }
}

/// Image de test : damier coloré avec une barre de titre (prévisualisation, tests).
pub fn sample_window(width: u32, height: u32) -> Image {
    let mut img = Image::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let px = if y < 28 {
                0xff1f2732
            } else if ((x / 32) + (y / 32)) % 2 == 0 {
                0xff5ccfe6
            } else {
                0xff2b3644
            };
            img.pixels[(y * width + x) as usize] = px;
        }
    }
    img
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: (u32, u32) = (800, 600);

    fn anim(effect: Effect, reverse: bool) -> Animation {
        Animation {
            effect,
            source: sample_window(320, 200),
            window: Rect {
                left: 240,
                top: 120,
                right: 560,
                bottom: 320,
            },
            target: Rect {
                left: 100,
                top: 566,
                right: 260,
                bottom: 600,
            },
            edge: Edge::Bottom,
            reverse,
            intensity: 0.7,
        }
    }

    fn frame(a: &Animation, t: f32) -> Image {
        let mut out = Image::new(SCREEN.0, SCREEN.1);
        a.render(t, &mut out);
        out
    }

    #[test]
    fn curves_start_at_zero_and_settle_at_one() {
        for c in [Curve::EaseOut, Curve::EaseInOut, Curve::Spring] {
            assert!(ease(c, 0.0).abs() < 1e-6, "{c:?}");
            assert!((ease(c, 1.0) - 1.0).abs() < 0.01, "{c:?}");
        }
        let overshoot = (0..100)
            .map(|i| ease(Curve::Spring, i as f32 / 100.0))
            .fold(0.0f32, f32::max);
        assert!(overshoot > 1.0, "le ressort dépasse avant de se stabiliser");
    }

    #[test]
    fn genie_starts_as_the_window_and_ends_inside_its_bar_button() {
        let a = anim(Effect::Genie, false);
        let start = frame(&a, 0.0);
        assert_eq!(start.bounds(), Some(a.window));
        assert_eq!(start.coverage(), 320 * 200);
        let mid = frame(&a, 0.5).bounds().unwrap();
        assert!(mid.bottom > a.window.bottom, "en route vers la barre : {mid:?}");
        let end = frame(&a, 1.0);
        assert_eq!(end.coverage(), 0, "aspirée entièrement");
        let late = frame(&a, 0.8).bounds().unwrap();
        assert!(
            late.left >= a.target.left - 40 && late.right <= a.target.right + 40,
            "{late:?}"
        );
    }

    #[test]
    fn genie_works_towards_a_left_bar() {
        let mut a = anim(Effect::Genie, false);
        a.edge = Edge::Left;
        a.target = Rect {
            left: 0,
            top: 200,
            right: 48,
            bottom: 248,
        };
        let mid = frame(&a, 0.6).bounds().unwrap();
        assert!(mid.left < a.window.left, "aspirée vers la gauche : {mid:?}");
        assert!(mid.right <= a.window.right);
    }

    #[test]
    fn restore_is_the_reverse_genie() {
        let a = anim(Effect::Genie, true);
        assert_eq!(frame(&a, 0.0).coverage(), 0);
        assert_eq!(frame(&a, 1.0).bounds(), Some(a.window));
    }

    #[test]
    fn jelly_grows_overshoots_and_settles_on_the_window() {
        let a = anim(Effect::Jelly, false);
        assert_eq!(frame(&a, 0.0).coverage(), 0, "invisible au départ");
        let settled = frame(&a, 1.0).bounds().unwrap();
        assert!(
            (settled.left - a.window.left).abs() <= 2 && (settled.right - a.window.right).abs() <= 2,
            "{settled:?}"
        );
        let widths: Vec<i32> = (5..20)
            .map(|i| frame(&a, i as f32 / 20.0).bounds().map(|b| b.width()).unwrap_or(0))
            .collect();
        assert!(
            widths.iter().any(|w| *w > a.window.width() + 4),
            "la gélatine déborde un instant : {widths:?}"
        );
    }

    #[test]
    fn zoom_fades_out_while_shrinking() {
        let a = anim(Effect::Zoom, false);
        let b = frame(&a, 0.6).bounds().unwrap();
        assert!(b.width() < a.window.width());
        assert_eq!(frame(&a, 1.0).coverage(), 0);
    }

    #[test]
    fn frames_never_panic_on_edges_of_the_screen_or_tiny_windows() {
        for effect in Effect::ALL {
            let mut a = anim(effect, false);
            a.window = Rect {
                left: -100,
                top: 550,
                right: 900,
                bottom: 700,
            };
            a.source = sample_window(1000, 150);
            for i in 0..=10 {
                frame(&a, i as f32 / 10.0);
            }
            a.source = sample_window(1, 1);
            a.window = Rect {
                left: 10,
                top: 10,
                right: 11,
                bottom: 11,
            };
            frame(&a, 0.5);
        }
    }

    #[test]
    fn config_is_clamped_and_old_files_load() {
        let c = FxConfig {
            duration_ms: 5,
            intensity: 250,
            ..Default::default()
        }
        .sanitized();
        assert_eq!((c.duration_ms, c.intensity), (DURATION_MIN, 100));
        let c: FxConfig = serde_json::from_str(r#"{"enabled":true}"#).unwrap();
        assert_eq!(c.minimize, Effect::Genie);
    }

    /// Coût du rendu : `cargo test --release -p prism-core fx_bench -- --ignored --nocapture`.
    #[test]
    #[ignore = "mesure de performance"]
    fn fx_bench() {
        let a = |effect| Animation {
            effect,
            source: sample_window(1280, 720),
            window: Rect {
                left: 320,
                top: 180,
                right: 1600,
                bottom: 900,
            },
            target: Rect {
                left: 200,
                top: 1040,
                right: 400,
                bottom: 1080,
            },
            edge: Edge::Bottom,
            reverse: false,
            intensity: 0.7,
        };
        let mut out = Image::new(1920, 1080);
        for effect in [Effect::Genie, Effect::Jelly, Effect::Zoom] {
            let anim = a(effect);
            let frames = 60;
            let t0 = std::time::Instant::now();
            let mut worst = std::time::Duration::ZERO;
            for i in 0..frames {
                let f0 = std::time::Instant::now();
                anim.render(i as f32 / frames as f32, &mut out);
                worst = worst.max(f0.elapsed());
            }
            let avg = t0.elapsed() / frames;
            println!(
                "{:<6} moyenne {:>6.2} ms  pire {:>6.2} ms par image (fenêtre 1280x720, écran 1920x1080)",
                format!("{effect:?}"),
                avg.as_secs_f64() * 1e3,
                worst.as_secs_f64() * 1e3
            );
        }
    }

    /// Prévisualisation : `cargo test -p prism-core fx_preview -- --ignored` écrit les
    /// images des animations dans `target/fx-frames/` (format PPM).
    #[test]
    #[ignore = "outil de prévisualisation"]
    fn fx_preview() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/fx-frames");
        std::fs::create_dir_all(&dir).unwrap();
        for (name, a) in [
            ("genie", anim(Effect::Genie, false)),
            ("jelly", anim(Effect::Jelly, false)),
            ("zoom", anim(Effect::Zoom, false)),
        ] {
            for i in 0..=8 {
                let f = frame(&a, i as f32 / 8.0);
                let mut ppm = format!("P6 {} {} 255\n", f.width, f.height).into_bytes();
                for px in &f.pixels {
                    let a8 = px >> 24;
                    // Fond sombre sous les pixels transparents.
                    let mix = |c: u32, bg: u32| (c + bg * (255 - a8) / 255).min(255) as u8;
                    ppm.extend([
                        mix((px >> 16) & 0xff, 0x14),
                        mix((px >> 8) & 0xff, 0x1a),
                        mix(px & 0xff, 0x24),
                    ]);
                }
                std::fs::write(dir.join(format!("{name}-{i}.ppm")), ppm).unwrap();
            }
        }
    }
}
