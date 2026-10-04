//! Logo Prism (losange bleu-violet), partagé par l'appli et le programme d'installation.

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Stroke};

/// Logo Prism : un prisme en losange (double pyramide allongée, comme le « plumbob »),
/// facettes dégradées du bleu au violet, qui tourne sur lui-même. `build` : secondes
/// depuis l'ouverture — les facettes apparaissent l'une après l'autre (séquence) ;
/// `done` : un faisceau de lumière le traverse et se décompose en spectre.
pub fn draw_prism(p: &egui::Painter, c: Pos2, size: f32, angle: f32, build: f32, done: bool, t: f32) {
    let blue = (0x3b_u8, 0x82_u8, 0xf6_u8);
    let violet = (0x8b_u8, 0x5c_u8, 0xf6_u8);
    let _ = (blue, violet);
    let mix = |a: (u8, u8, u8), b: (u8, u8, u8), k: f32, light: f32| {
        let m = |x: u8, y: u8| ((x as f32 + (y as f32 - x as f32) * k) * light).clamp(0.0, 255.0) as u8;
        (m(a.0, b.0), m(a.1, b.1), m(a.2, b.2))
    };
    // Halo qui respire, bleu-violet.
    let breath = 0.85 + 0.15 * (t * 1.6).sin();
    for i in 0..6 {
        let r = size * (0.7 + i as f32 * 0.14) * breath;
        let a = (12.0 - i as f32 * 2.0).max(0.0) as u8;
        let (r0, g0, b0) = mix(blue, violet, i as f32 / 6.0, 1.0);
        p.circle_filled(c, r, Color32::from_rgba_unmultiplied(r0, g0, b0, a));
    }
    // Géométrie : anneau de 4 sommets au milieu, une pointe en haut, une en bas.
    let ring_r = size * 0.62;
    let (top_h, bot_h) = (size * 1.18, size * 1.18);
    let tilt: f32 = 0.18;
    let project = |x: f32, y: f32, z: f32| -> (Pos2, f32) {
        let (sy, cy) = tilt.sin_cos();
        let y2 = y * cy - z * sy;
        let z2 = y * sy + z * cy;
        let k = 520.0 / (520.0 + z2);
        (Pos2::new(c.x + x * k, c.y + y2 * k), z2)
    };
    let ring: Vec<(Pos2, f32)> = (0..4)
        .map(|k| {
            let a = angle + k as f32 * std::f32::consts::FRAC_PI_2;
            project(a.cos() * ring_r, 0.0, a.sin() * ring_r)
        })
        .collect();
    let top = project(0.0, -top_h, 0.0);
    let bot = project(0.0, bot_h, 0.0);
    // 8 facettes ; visibles si leur contour projeté tourne dans le bon sens.
    let mut faces: Vec<(f32, [Pos2; 3], Color32, f32)> = Vec::new();
    for k in 0..4 {
        let j = (k + 1) % 4;
        for (i, (apex, a, b)) in [(top, ring[k], ring[j]), (bot, ring[j], ring[k])]
            .into_iter()
            .enumerate()
        {
            let tri = [apex.0, a.0, b.0];
            let cross = (tri[1].x - tri[0].x) * (tri[2].y - tri[0].y) - (tri[1].y - tri[0].y) * (tri[2].x - tri[0].x);
            if cross >= 0.0 {
                continue;
            }
            // Éclairage : facettes face à la lumière (haut gauche) plus claires.
            let mid_angle = angle + (k as f32 + 0.5) * std::f32::consts::FRAC_PI_2;
            let facing = (-mid_angle.sin()).max(0.0);
            let side = (mid_angle.cos() * -0.5 + 0.5).clamp(0.0, 1.0);
            // Haut : bleu clair → bleu profond ; bas : violet → violet profond, selon le
            // côté (contraste entre facettes : effet cristal).
            let light = 0.72 + 0.5 * facing;
            let (from, to) = if i == 0 {
                ((0x93_u8, 0xc5_u8, 0xfd_u8), (0x25_u8, 0x63_u8, 0xeb_u8))
            } else {
                ((0xa7_u8, 0x8b_u8, 0xfa_u8), (0x5b_u8, 0x21_u8, 0xb6_u8))
            };
            let (r0, g0, b0) = mix(from, to, side, light);
            let depth = (a.1 + b.1) / 2.0;
            let reveal = ((build - 0.12 * (k * 2 + i) as f32) / 0.3).clamp(0.0, 1.0);
            faces.push((depth, tri, Color32::from_rgb(r0, g0, b0), reveal));
        }
    }
    faces.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    for (_, tri, col, a) in &faces {
        if *a > 0.0 {
            let fill = Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), (245.0 * a) as u8);
            p.add(egui::Shape::convex_polygon(tri.to_vec(), fill, Stroke::NONE));
        }
    }
    // Arêtes fines et lumineuses des facettes visibles, après les facettes.
    let edge_a = ((build - 1.1) / 0.4).clamp(0.0, 1.0);
    if edge_a > 0.0 {
        let s = Stroke::new(
            1.2,
            Color32::from_rgba_unmultiplied(220, 230, 255, (150.0 * edge_a) as u8),
        );
        for (_, tri, _, a) in &faces {
            if *a >= 1.0 {
                p.line_segment([tri[0], tri[1]], s);
                p.line_segment([tri[1], tri[2]], s);
                p.line_segment([tri[2], tri[0]], s);
            }
        }
    }
    // Reflet : un éclat qui glisse sur la facette avant.
    let glint = ((t * 0.8).sin() * 0.5 + 0.5) * edge_a;
    if glint > 0.05 {
        p.circle_filled(
            Pos2::new(c.x - size * 0.18, c.y - size * 0.45),
            size * 0.06,
            Color32::from_rgba_unmultiplied(255, 255, 255, (90.0 * glint) as u8),
        );
    }
    // Fin : un faisceau blanc entre à gauche et ressort décomposé à droite.
    if done {
        let a = (t / 0.8).clamp(0.0, 1.0);
        let entry = Pos2::new(c.x - size * 2.2, c.y + size * 0.1);
        let hit = Pos2::new(c.x - size * 0.4, c.y);
        p.line_segment(
            [entry, hit],
            Stroke::new(3.0, Color32::from_rgba_unmultiplied(255, 255, 255, (230.0 * a) as u8)),
        );
        let rainbow = [
            (0xff, 0x4d, 0x4d),
            (0xff, 0x9f, 0x40),
            (0xff, 0xe0, 0x4d),
            (0x5c, 0xe6, 0x7a),
            (0x3b, 0x82, 0xf6),
            (0x8b, 0x5c, 0xf6),
        ];
        let out = Pos2::new(c.x + size * 0.4, c.y);
        for (i, (r, g, b)) in rainbow.iter().enumerate() {
            let spread = (i as f32 - 2.5) * 0.09;
            let end = Pos2::new(c.x + size * 2.4, c.y + size * (0.15 + spread * 6.0));
            let e = Pos2::new(out.x + (end.x - out.x) * a, out.y + (end.y - out.y) * a);
            p.line_segment([out, e], Stroke::new(2.4, Color32::from_rgb(*r, *g, *b)));
        }
    }
    let _ = (Align2::CENTER_CENTER, FontId::default());
}
