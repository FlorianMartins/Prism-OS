//! Logo Prism, partagé par l'appli et le programme d'installation : un octaèdre en fil
//! de lumière (arêtes fines cyan → violet, faces de verre sombre), un anneau orbital fin
//! et un trait de balayage. Épuré : ni halo flou, ni dégradés lourds, ni arc-en-ciel.

use eframe::egui::{self, Color32, Pos2, Shape, Stroke};

const CYAN: (f32, f32, f32) = (34.0, 211.0, 238.0);
const VIOLET: (f32, f32, f32) = (167.0, 139.0, 250.0);

fn mix(k: f32, alpha: f32) -> Color32 {
    let k = k.clamp(0.0, 1.0);
    let m = |a: f32, b: f32| (a + (b - a) * k) as u8;
    Color32::from_rgba_unmultiplied(
        m(CYAN.0, VIOLET.0),
        m(CYAN.1, VIOLET.1),
        m(CYAN.2, VIOLET.2),
        (alpha.clamp(0.0, 1.0) * 255.0) as u8,
    )
}

fn lerp(a: Pos2, b: Pos2, k: f32) -> Pos2 {
    Pos2::new(a.x + (b.x - a.x) * k, a.y + (b.y - a.y) * k)
}

/// Dessine le logo centré en `c` (`size` ≈ demi-hauteur / 1,15).
/// `angle` : rotation ; `build` : secondes depuis l'apparition (les arêtes se tracent
/// une à une, puis les faces apparaissent) ; `done` : balayage lumineux de fin
/// d'installation ; `t` : horloge (anneau, balayage).
pub fn draw_prism(p: &egui::Painter, c: Pos2, size: f32, angle: f32, build: f32, done: bool, t: f32) {
    let ring_r = size * 0.66;
    let h = size * 1.15;
    let tilt: f32 = 0.24;
    let (st, ct) = tilt.sin_cos();
    // Projection avec légère perspective ; rend le point et sa profondeur (z > 0 : loin).
    let project = |x: f32, y: f32, z: f32| -> (Pos2, f32) {
        let y2 = y * ct - z * st;
        let z2 = y * st + z * ct;
        let k = 6.0 * size / (6.0 * size + z2);
        (Pos2::new(c.x + x * k, c.y + y2 * k), z2)
    };
    let ring: Vec<(Pos2, f32)> = (0..4)
        .map(|k| {
            let a = angle + k as f32 * std::f32::consts::FRAC_PI_2;
            project(a.cos() * ring_r, 0.0, a.sin() * ring_r)
        })
        .collect();
    let top = project(0.0, -h, 0.0);
    let bot = project(0.0, h, 0.0);
    let large = size >= 24.0;

    // Anneau orbital : moitié arrière avant le prisme, moitié avant après.
    let orbit = |front: bool| {
        if !large {
            return;
        }
        let reveal = ((build - 0.9) / 0.6).clamp(0.0, 1.0);
        if reveal <= 0.0 {
            return;
        }
        let (rx, ry) = (size * 1.62, size * 0.44);
        let n = 64;
        let pts: Vec<(Pos2, bool)> = (0..=n)
            .map(|i| {
                let a = i as f32 / n as f32 * std::f32::consts::TAU;
                let (s, co) = a.sin_cos();
                // Ellipse légèrement inclinée (−8°).
                let (x, y) = (co * rx, s * ry);
                let (si, ci) = (-0.14f32).sin_cos();
                (Pos2::new(c.x + x * ci - y * si, c.y + x * si + y * ci), s > 0.0)
            })
            .collect();
        for w in pts.windows(2) {
            let (a, fa) = w[0];
            let (b, _) = w[1];
            if fa == front {
                let alpha = if front { 0.55 } else { 0.18 } * reveal;
                p.line_segment([a, b], Stroke::new(1.0, mix(0.5, alpha)));
            }
        }
        // Point lumineux qui parcourt l'anneau.
        let a = t * 0.9;
        let (s, co) = a.sin_cos();
        if (s > 0.0) == front {
            let (x, y) = (co * rx, s * ry);
            let (si, ci) = (-0.14f32).sin_cos();
            let dot = Pos2::new(c.x + x * ci - y * si, c.y + x * si + y * ci);
            p.circle_filled(dot, 2.2, mix(0.1, 0.95 * reveal));
            p.circle_filled(dot, 5.0, mix(0.1, 0.12 * reveal));
        }
    };
    orbit(false);

    // Faces de verre sombre (visibles seulement), éclaircies côté lumière.
    let faces_reveal = ((build - 1.0) / 0.5).clamp(0.0, 1.0);
    let mut faces: Vec<(f32, [Pos2; 3], f32, f32)> = Vec::new();
    for k in 0..4 {
        let j = (k + 1) % 4;
        for (half, apex) in [(0.0f32, top), (1.0f32, bot)] {
            let (a, b) = if half == 0.0 {
                (ring[k], ring[j])
            } else {
                (ring[j], ring[k])
            };
            let tri = [apex.0, a.0, b.0];
            let cross = (tri[1].x - tri[0].x) * (tri[2].y - tri[0].y) - (tri[1].y - tri[0].y) * (tri[2].x - tri[0].x);
            // Face de dos, ou vue presque de profil : un triangle quasi plat fait
            // partir le lissage des bords en pointe démesurée (vu à l'écran).
            if cross >= -size * size * 0.05 {
                continue;
            }
            let mid = angle + (k as f32 + 0.5) * std::f32::consts::FRAC_PI_2;
            let light = (0.5 - 0.5 * mid.sin()).clamp(0.0, 1.0);
            faces.push(((a.1 + b.1) / 2.0, tri, half, light));
        }
    }
    faces.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    if faces_reveal > 0.0 {
        for (_, tri, half, light) in &faces {
            let base = Color32::from_rgba_unmultiplied(12, 18, 36, (200.0 * faces_reveal) as u8);
            p.add(Shape::convex_polygon(tri.to_vec(), base, Stroke::NONE));
            let tint = mix(*half, (0.10 + 0.22 * light) * faces_reveal);
            p.add(Shape::convex_polygon(tri.to_vec(), tint, Stroke::NONE));
        }
    }

    // Arêtes : les 12, celles de derrière en filigrane ; tracées une à une au début.
    let mut edges: Vec<(Pos2, Pos2, f32, f32)> = Vec::new(); // (a, b, couleur, profondeur)
    for k in 0..4 {
        let j = (k + 1) % 4;
        edges.push((top.0, ring[k].0, 0.0, ring[k].1));
        edges.push((ring[k].0, ring[j].0, 0.5, (ring[k].1 + ring[j].1) / 2.0));
        edges.push((ring[k].0, bot.0, 1.0, ring[k].1));
    }
    for (i, (a, b, col, depth)) in edges.iter().enumerate() {
        let k = ((build - i as f32 * 0.06) / 0.28).clamp(0.0, 1.0);
        if k <= 0.0 {
            continue;
        }
        let back = *depth > 0.0;
        let alpha = if back { 0.22 } else { 0.95 };
        let width = if back { 0.8 } else { (size / 14.0).clamp(1.0, 1.8) };
        let end = lerp(*a, *b, k);
        p.line_segment([*a, end], Stroke::new(width, mix(*col, alpha)));
    }
    // Sommets avant : petits points lumineux.
    if large && faces_reveal > 0.0 {
        for (v, z) in ring.iter().chain([&top, &bot]) {
            if *z <= 0.0 {
                p.circle_filled(*v, 1.6, mix(0.2, 0.9 * faces_reveal));
            }
        }
    }

    // Balayage : un trait horizontal fin descend à travers le prisme (en continu une
    // fois construit dans l'installateur, une fois par cycle de 3 s).
    if large && (done || build > 1.6) {
        let cycle = (t / 3.0).fract();
        let y = c.y - h + cycle * 2.0 * h;
        let dy = (y - c.y).abs() / h;
        if dy < 1.0 {
            let w = ring_r * (1.0 - dy);
            let fade = (1.0 - dy).powf(0.6) * if done { 0.9 } else { 0.5 };
            p.line_segment(
                [Pos2::new(c.x - w, y), Pos2::new(c.x + w, y)],
                Stroke::new(
                    1.2,
                    Color32::from_rgba_unmultiplied(220, 250, 255, (200.0 * fade) as u8),
                ),
            );
        }
    }

    orbit(true);
}
