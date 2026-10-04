//! Gélatine pendant le déplacement d'une fenêtre (« wobbly windows »).
//!
//! Une grille de points suit la fenêtre au bout de ressorts amortis : le point saisi
//! reste exactement sous le curseur, les parties éloignées suivent avec du retard,
//! dépassent à l'arrêt puis se posent. Le rendu déforme la copie de la fenêtre sur un
//! maillage fin (triangles texturés), en logiciel, comme le reste de `fx`.

use crate::fx::Image;

/// Points de contrôle par côté.
const NODES: usize = 8;
/// Subdivision de chaque case de la grille pour le rendu.
const SUB: usize = 4;
/// Pas d'intégration maximal (s) : stable même si une image arrive en retard.
const STEP: f32 = 0.002;

#[derive(Clone, Debug)]
struct Node {
    /// Écart à la position au repos (repère écran) et sa vitesse.
    off: (f32, f32),
    vel: (f32, f32),
    k: f32,
    c: f32,
}

/// État de la gélatine d'une fenêtre en cours de déplacement.
#[derive(Clone, Debug)]
pub struct Wobbly {
    width: f32,
    height: f32,
    grab: (f32, f32),
    origin: (f32, f32),
    nodes: Vec<Node>,
    max_off: f32,
}

impl Wobbly {
    /// `origin` : coin haut-gauche de la fenêtre à l'écran ; `grab` : point saisi,
    /// relatif à la fenêtre ; `intensity` : 0 (raide) à 1 (très mou).
    pub fn new(width: u32, height: u32, origin: (f32, f32), grab: (f32, f32), intensity: f32) -> Wobbly {
        let (w, h) = (width.max(1) as f32, height.max(1) as f32);
        let i = intensity.clamp(0.0, 1.0);
        let diag = (w * w + h * h).sqrt();
        // Pulsation propre loin du point saisi (rad/s) et amortissement.
        // À 70 % : ≈ 3 Hz loin du point saisi, le bord éloigné traîne d'environ 40 px
        // à 1 000 px/s et se pose en un peu plus d'une demi-seconde.
        let omega_far = 34.0 - 20.0 * i;
        let omega_near = 70.0;
        let zeta = 0.5 - 0.28 * i;
        let mut nodes = Vec::with_capacity(NODES * NODES);
        for r in 0..NODES {
            for col in 0..NODES {
                let rest = (w * col as f32 / (NODES - 1) as f32, h * r as f32 / (NODES - 1) as f32);
                let d = ((rest.0 - grab.0).powi(2) + (rest.1 - grab.1).powi(2)).sqrt() / diag;
                let near = (-(d * d) / 0.08).exp();
                let omega = omega_far + (omega_near - omega_far) * near;
                nodes.push(Node {
                    off: (0.0, 0.0),
                    vel: (0.0, 0.0),
                    k: omega * omega,
                    c: 2.0 * zeta * omega,
                });
            }
        }
        Wobbly {
            width: w,
            height: h,
            grab: (grab.0.clamp(0.0, w), grab.1.clamp(0.0, h)),
            origin,
            nodes,
            max_off: (0.18 * w.max(h) + 40.0).min(260.0),
        }
    }

    /// Écart maximal d'un point par rapport à la fenêtre (marge de la couche d'affichage).
    pub fn margin(&self) -> i32 {
        self.max_off.ceil() as i32 + 2
    }

    /// Fait avancer la simulation : la fenêtre est maintenant en `origin`.
    pub fn step(&mut self, origin: (f32, f32), dt: f32) {
        // La fenêtre a sauté de `delta` : chaque point garde sa position à l'écran,
        // donc son écart au repos recule d'autant ; les ressorts le ramènent ensuite.
        let delta = (origin.0 - self.origin.0, origin.1 - self.origin.1);
        self.origin = origin;
        let m = self.max_off;
        for n in &mut self.nodes {
            n.off.0 = (n.off.0 - delta.0).clamp(-m, m);
            n.off.1 = (n.off.1 - delta.1).clamp(-m, m);
        }
        let mut left = dt.clamp(0.0, 0.1);
        while left > 0.0 {
            let h = left.min(STEP);
            left -= h;
            for n in &mut self.nodes {
                // Euler semi-implicite : vitesse d'abord, stable pour un ressort amorti.
                n.vel.0 += (-n.k * n.off.0 - n.c * n.vel.0) * h;
                n.vel.1 += (-n.k * n.off.1 - n.c * n.vel.1) * h;
                n.off.0 = (n.off.0 + n.vel.0 * h).clamp(-m, m);
                n.off.1 = (n.off.1 + n.vel.1 * h).clamp(-m, m);
            }
        }
    }

    /// La gélatine est posée : la vraie fenêtre peut reprendre sa place.
    pub fn settled(&self) -> bool {
        self.nodes
            .iter()
            .all(|n| n.off.0.abs() < 0.5 && n.off.1.abs() < 0.5 && n.vel.0.abs() < 8.0 && n.vel.1.abs() < 8.0)
    }

    /// Plus grand écart actuel d'un point (tests, mesures).
    pub fn max_offset(&self) -> f32 {
        self.nodes.iter().map(|n| n.off.0.hypot(n.off.1)).fold(0.0, f32::max)
    }

    /// Poids de la déformation : nul au point saisi (qui reste sous le curseur),
    /// plein loin de lui.
    fn weight(&self, x: f32, y: f32) -> f32 {
        let diag = (self.width * self.width + self.height * self.height).sqrt();
        let d = ((x - self.grab.0).powi(2) + (y - self.grab.1).powi(2)).sqrt() / diag;
        let t = (d / 0.45).min(1.0);
        t * t * (3.0 - 2.0 * t)
    }

    /// Position affichée du point (x, y) de la fenêtre, dans le repère de `out`.
    fn point(&self, x: f32, y: f32, at: (f32, f32)) -> (f32, f32) {
        let fx = (x / self.width * (NODES - 1) as f32).clamp(0.0, (NODES - 1) as f32);
        let fy = (y / self.height * (NODES - 1) as f32).clamp(0.0, (NODES - 1) as f32);
        let (c0, r0) = ((fx as usize).min(NODES - 2), (fy as usize).min(NODES - 2));
        let (tx, ty) = (fx - c0 as f32, fy - r0 as f32);
        let o = |r: usize, c: usize| self.nodes[r * NODES + c].off;
        let (a, b, c, d) = (o(r0, c0), o(r0, c0 + 1), o(r0 + 1, c0), o(r0 + 1, c0 + 1));
        let ox = (a.0 * (1.0 - tx) + b.0 * tx) * (1.0 - ty) + (c.0 * (1.0 - tx) + d.0 * tx) * ty;
        let oy = (a.1 * (1.0 - tx) + b.1 * tx) * (1.0 - ty) + (c.1 * (1.0 - tx) + d.1 * tx) * ty;
        let w = self.weight(x, y);
        (at.0 + x + ox * w, at.1 + y + oy * w)
    }

    /// Dessine la fenêtre déformée. `at` : coin de la fenêtre au repos dans le repère
    /// de `out`. `out` est effacée d'abord.
    pub fn render(&self, src: &Image, at: (f32, f32), out: &mut Image) {
        out.clear();
        if self.max_offset() < 0.5 {
            // Au repos : simple copie.
            let (ax, ay) = (at.0.round() as i32, at.1.round() as i32);
            for y in 0..src.height {
                let oy = ay + y as i32;
                if oy < 0 || oy >= out.height as i32 {
                    continue;
                }
                for x in 0..src.width {
                    let ox = ax + x as i32;
                    if ox >= 0 && ox < out.width as i32 {
                        out.pixels[(oy as u32 * out.width + ox as u32) as usize] =
                            src.pixels[(y * src.width + x) as usize];
                    }
                }
            }
            return;
        }
        let cells = (NODES - 1) * SUB;
        let mut grid = Vec::with_capacity((cells + 1) * (cells + 1));
        for r in 0..=cells {
            for c in 0..=cells {
                let (x, y) = (
                    self.width * c as f32 / cells as f32,
                    self.height * r as f32 / cells as f32,
                );
                let p = self.point(x, y, at);
                let uv = (x / self.width * src.width as f32, y / self.height * src.height as f32);
                grid.push((p, uv));
            }
        }
        let v = |r: usize, c: usize| grid[r * (cells + 1) + c];
        for r in 0..cells {
            for c in 0..cells {
                let (a, b, cc, d) = (v(r, c), v(r, c + 1), v(r + 1, c), v(r + 1, c + 1));
                triangle(src, out, [a, b, d]);
                triangle(src, out, [a, d, cc]);
            }
        }
    }
}

type Vertex = ((f32, f32), (f32, f32));

/// Triangle texturé (échantillonnage au plus proche, coordonnées affines).
fn triangle(src: &Image, out: &mut Image, t: [Vertex; 3]) {
    let [(p0, t0), (p1, t1), (p2, t2)] = t;
    let area = (p1.0 - p0.0) * (p2.1 - p0.1) - (p1.1 - p0.1) * (p2.0 - p0.0);
    if area.abs() < 1e-6 {
        return;
    }
    // Signe choisi pour que chaque poids vaille 1 sur son sommet.
    let inv = -1.0 / area;
    let min_x = p0.0.min(p1.0).min(p2.0).floor().max(0.0) as i32;
    let max_x = (p0.0.max(p1.0).max(p2.0).ceil() as i32).min(out.width as i32 - 1);
    let min_y = p0.1.min(p1.1).min(p2.1).floor().max(0.0) as i32;
    let max_y = (p0.1.max(p1.1).max(p2.1).ceil() as i32).min(out.height as i32 - 1);
    if min_x > max_x || min_y > max_y {
        return;
    }
    // Fonctions d'arête normalisées (coordonnées barycentriques), pas en x et en y.
    let edge = |a: (f32, f32), b: (f32, f32)| {
        let (dx, dy) = ((b.1 - a.1) * inv, (a.0 - b.0) * inv);
        let at = move |x: f32, y: f32| ((x - a.0) * (b.1 - a.1) - (y - a.1) * (b.0 - a.0)) * inv;
        (dx, dy, at)
    };
    let (d0x, d0y, e0) = edge(p1, p2);
    let (d1x, d1y, e1) = edge(p2, p0);
    let (d2x, d2y, e2) = edge(p0, p1);
    let (sx, sy) = (min_x as f32 + 0.5, min_y as f32 + 0.5);
    let (mut r0, mut r1, mut r2) = (e0(sx, sy), e1(sx, sy), e2(sx, sy));
    let (sw, sh) = (src.width as f32 - 1.0, src.height as f32 - 1.0);
    const EPS: f32 = -1e-4;
    for y in min_y..=max_y {
        let (mut w0, mut w1, mut w2) = (r0, r1, r2);
        let row = (y as u32 * out.width) as usize;
        for x in min_x..=max_x {
            if w0 >= EPS && w1 >= EPS && w2 >= EPS {
                let u = (w0 * t0.0 + w1 * t1.0 + w2 * t2.0).clamp(0.0, sw);
                let v = (w0 * t0.1 + w1 * t1.1 + w2 * t2.1).clamp(0.0, sh);
                out.pixels[row + x as usize] = src.pixels[(v as u32 * src.width + u as u32) as usize];
            }
            w0 += d0x;
            w1 += d1x;
            w2 += d2x;
        }
        r0 += d0y;
        r1 += d1y;
        r2 += d2y;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::sample_window;

    const DT: f32 = 1.0 / 60.0;

    fn frame(w: &Wobbly, src: &Image, at: (f32, f32)) -> Image {
        let mut out = Image::new(900, 600);
        w.render(src, at, &mut out);
        out
    }

    #[test]
    fn at_rest_it_is_exactly_the_window() {
        let src = sample_window(320, 200);
        let w = Wobbly::new(320, 200, (100.0, 100.0), (160.0, 10.0), 0.7);
        let out = frame(&w, &src, (100.0, 100.0));
        assert_eq!(out.coverage(), 320 * 200);
        assert!(w.settled());
    }

    #[test]
    fn the_far_side_lags_and_the_grabbed_point_stays_under_the_cursor() {
        let src = sample_window(320, 200);
        // Saisie par la barre de titre, à gauche ; on tire vers la droite.
        let mut w = Wobbly::new(320, 200, (100.0, 100.0), (20.0, 10.0), 0.7);
        for i in 1..=6 {
            w.step((100.0 + 25.0 * i as f32, 100.0), DT);
        }
        let at = (250.0, 100.0);
        let out = frame(&w, &src, at);
        let b = out.bounds().unwrap();
        assert!(b.right < 250 + 320 - 8, "le bord éloigné traîne : {b:?}");
        // Le pixel saisi est toujours celui de la source.
        let px = |img: &Image, x: u32, y: u32| img.pixels[(y * img.width + x) as usize];
        assert_eq!(px(&out, 250 + 20, 100 + 10), px(&src, 20, 10));
    }

    #[test]
    fn it_overshoots_when_the_window_stops_then_settles() {
        let mut w = Wobbly::new(320, 200, (100.0, 100.0), (20.0, 10.0), 0.7);
        for i in 1..=10 {
            w.step((100.0 + 30.0 * i as f32, 100.0), DT);
        }
        // Arrêt : le bord droit, en retard (écart négatif), doit dépasser (écart positif).
        let far = NODES - 1; // ligne du haut, colonne de droite
        let mut overshoot = false;
        let mut frames = 0;
        while !w.settled() {
            w.step((400.0, 100.0), DT);
            overshoot |= w.nodes[far].off.0 > 1.0;
            frames += 1;
            assert!(frames < 120, "posée en moins de 2 s");
        }
        assert!(overshoot, "la gélatine dépasse avant de se poser");
    }

    #[test]
    fn displacement_is_bounded_even_for_a_violent_throw() {
        let mut w = Wobbly::new(400, 300, (0.0, 0.0), (200.0, 10.0), 1.0);
        w.step((3000.0, -2000.0), DT);
        assert!(w.max_offset() <= w.margin() as f32 * 1.5);
        let src = sample_window(400, 300);
        let mut out = Image::new(1200, 1000);
        w.render(&src, (400.0, 350.0), &mut out);
    }

    #[test]
    fn tiny_or_offscreen_windows_never_panic() {
        let src = sample_window(1, 1);
        let mut w = Wobbly::new(1, 1, (0.0, 0.0), (5.0, 5.0), 0.5);
        w.step((50.0, 50.0), DT);
        frame(&w, &src, (10.0, 10.0));
        let src = sample_window(500, 400);
        let mut w = Wobbly::new(500, 400, (0.0, 0.0), (250.0, 5.0), 0.5);
        w.step((80.0, 0.0), DT);
        frame(&w, &src, (-300.0, 400.0));
        frame(&w, &src, (800.0, -390.0));
    }

    /// `cargo test --release -p prism-core wobbly_bench -- --ignored --nocapture`.
    #[test]
    #[ignore = "mesure de performance"]
    fn wobbly_bench() {
        let src = sample_window(1280, 720);
        let mut w = Wobbly::new(1280, 720, (300.0, 200.0), (300.0, 15.0), 0.7);
        let m = w.margin() as f32;
        let mut out = Image::new(1280 + 2 * m as u32, 720 + 2 * m as u32);
        let (mut total, mut worst) = (std::time::Duration::ZERO, std::time::Duration::ZERO);
        let frames = 120;
        for i in 0..frames {
            let x = 300.0 + 12.0 * (i.min(60)) as f32;
            let t0 = std::time::Instant::now();
            w.step((x, 200.0), DT);
            w.render(&src, (m, m), &mut out);
            let e = t0.elapsed();
            total += e;
            worst = worst.max(e);
        }
        println!(
            "gélatine au déplacement, fenêtre 1280x720 : moyenne {:.2} ms, pire {:.2} ms par image",
            total.as_secs_f64() * 1e3 / frames as f64,
            worst.as_secs_f64() * 1e3
        );
    }

    /// Prévisualisation : `cargo test -p prism-core wobbly_preview -- --ignored` écrit
    /// des images d'un déplacement puis d'un arrêt dans `target/fx-frames/` (PPM).
    #[test]
    #[ignore = "outil de prévisualisation"]
    fn wobbly_preview() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/fx-frames");
        std::fs::create_dir_all(&dir).unwrap();
        let src = sample_window(320, 200);
        let mut w = Wobbly::new(320, 200, (40.0, 150.0), (60.0, 10.0), 0.7);
        let mut x = 40.0;
        for i in 0..40 {
            if i < 12 {
                x += 28.0;
            }
            w.step((x, 150.0), DT);
            if i % 4 == 3 {
                let out = frame(&w, &src, (x, 150.0));
                let mut ppm = format!("P6 {} {} 255\n", out.width, out.height).into_bytes();
                for px in &out.pixels {
                    let a8 = px >> 24;
                    let mix = |c: u32, bg: u32| (c + bg * (255 - a8) / 255).min(255) as u8;
                    ppm.extend([
                        mix((px >> 16) & 0xff, 0x14),
                        mix((px >> 8) & 0xff, 0x1a),
                        mix(px & 0xff, 0x24),
                    ]);
                }
                std::fs::write(dir.join(format!("wobbly-{:02}.ppm", i)), ppm).unwrap();
            }
        }
    }
}
