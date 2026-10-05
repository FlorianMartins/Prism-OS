//! Logo Prism, partagé par l'appli et le programme d'installation : un triangle aux
//! coins arrondis découpé en trois facettes éclairées (une pyramide vue de dessus) —
//! cyan côté lumière, violet, indigo dans l'ombre — et deux arêtes de lumière. Même
//! géométrie que `installer/make_icons.py`, qui produit les icônes des exécutables.

use eframe::egui::{self, Color32, Mesh, Pos2, Shape, Stroke, Vec2};

const CYAN: Color32 = Color32::from_rgb(34, 211, 238);
const SKY: Color32 = Color32::from_rgb(56, 189, 248);
const VIOLET: Color32 = Color32::from_rgb(167, 139, 250);
const PURPLE: Color32 = Color32::from_rgb(139, 92, 246);
const INDIGO: Color32 = Color32::from_rgb(91, 84, 230);
const DEEP: Color32 = Color32::from_rgb(55, 48, 163);

/// Facettes, dans l'ordre des contours : droite (haut → bas droit), bas, gauche.
const FACETS: [(Color32, Color32); 3] = [(VIOLET, PURPLE), (INDIGO, DEEP), (CYAN, SKY)];

fn mix(a: Color32, b: Color32, t: f32, alpha: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgba_unmultiplied(
        m(a.r(), b.r()),
        m(a.g(), b.g()),
        m(a.b(), b.b()),
        (alpha.clamp(0.0, 1.0) * 255.0) as u8,
    )
}

/// Sommet de la pyramide, contours des trois facettes (de pointe à pointe, coins
/// arrondis) et pointes. `r` : rayon du cercle circonscrit.
pub fn geometry(c: Pos2, r: f32) -> (Pos2, [Vec<Pos2>; 3], [Pos2; 3]) {
    let round = 0.16 * r;
    // Centre de gravité plus bas que `c`, pour que la forme (pointe du haut raccourcie
    // par l'arrondi) soit centrée : haut à c − 0,67 r, bas à c + 0,67 r.
    let g = Pos2::new(c.x, c.y + 0.17 * r);
    let dir = |deg: f32| Vec2::angled(deg.to_radians());
    let inner: [Pos2; 3] = std::array::from_fn(|k| g + dir(-90.0 + 120.0 * k as f32) * (r - 2.0 * round));
    let tip_angle = |k: usize| (-90.0 + 120.0 * k as f32).to_radians();
    let arc = |i: usize, a0: f32, a1: f32| -> Vec<Pos2> {
        (0..=10)
            .map(|t| inner[i] + Vec2::angled(a0 + (a1 - a0) * t as f32 / 10.0) * round)
            .collect()
    };
    let chains = std::array::from_fn(|k| {
        let j = (k + 1) % 3;
        let normal = (-30.0 + 120.0 * k as f32).to_radians();
        let mut end = tip_angle(j);
        if end < normal {
            end += std::f32::consts::TAU;
        }
        let mut chain = arc(k, tip_angle(k), normal);
        chain.extend(arc(j, normal, end).into_iter().skip(1));
        chain
    });
    let tips = std::array::from_fn(|k| inner[k] + Vec2::angled(tip_angle(k)) * round);
    let apex = Pos2::new(g.x - 0.07 * r, g.y - 0.08 * r);
    (apex, chains, tips)
}

/// Découpe le polygone convexe `poly` par le demi-plan à gauche de a→b (Sutherland–Hodgman).
fn clip(poly: &[Pos2], a: Pos2, b: Pos2) -> Vec<Pos2> {
    let side = |p: Pos2| (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
    let mut out = Vec::new();
    for i in 0..poly.len() {
        let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
        let (sp, sq) = (side(p), side(q));
        if sp >= 0.0 {
            out.push(p);
        }
        if (sp >= 0.0) != (sq >= 0.0) {
            out.push(p + (q - p) * (sp / (sp - sq)));
        }
    }
    out
}

/// Dessine le logo centré en `c`. `r` : rayon (hauteur du logo ≈ 1,5 × r).
/// `build` : secondes depuis l'apparition (les facettes arrivent une à une) ;
/// `shine` : avancée (0 → 1) d'un reflet qui balaie le logo en diagonale, hors de
/// [0, 1] : pas de reflet.
pub fn draw_logo(p: &egui::Painter, c: Pos2, r: f32, build: f32, shine: f32) {
    let ease = |x: f32| 1.0 - (1.0 - x.clamp(0.0, 1.0)).powi(3);
    let r = r * (0.88 + 0.12 * ease(build / 0.5));
    let (apex, chains, tips) = geometry(c, r);
    for (k, chain) in chains.iter().enumerate() {
        let alpha = ease((build - 0.12 * k as f32) / 0.3);
        if alpha <= 0.0 {
            continue;
        }
        let (c0, c1) = FACETS[k];
        let mut mesh = Mesh::default();
        mesh.colored_vertex(apex, mix(c0, c1, 0.5, alpha));
        let n = chain.len();
        for (i, q) in chain.iter().enumerate() {
            mesh.colored_vertex(*q, mix(c0, c1, i as f32 / (n - 1) as f32, alpha));
        }
        for i in 1..n as u32 {
            mesh.add_triangle(0, i, i + 1);
        }
        p.add(Shape::mesh(mesh));
        // Bord lissé (un maillage n'est pas anticrénelé, un trait l'est).
        p.add(Shape::line(chain.clone(), Stroke::new(1.0, mix(c0, c1, 0.6, alpha))));
    }
    // Arêtes de lumière (côté éclairé), une fois les facettes posées.
    let ridge = ease((build - 0.45) / 0.3);
    if ridge > 0.0 && r >= 12.0 {
        let w = (r * 0.02).clamp(0.8, 2.0);
        for k in [0, 2] {
            let end = apex + (tips[k] - apex) * 0.96;
            p.line_segment(
                [apex, end],
                Stroke::new(w, Color32::from_white_alpha((70.0 * ridge) as u8)),
            );
        }
    }
    // Reflet : une bande claire qui traverse la silhouette en diagonale.
    if (0.0..=1.0).contains(&shine) {
        let outline: Vec<Pos2> = chains
            .iter()
            .flat_map(|ch| ch[..ch.len() - 1].iter().copied())
            .collect();
        let span = 2.4 * r;
        let x = c.x - span / 2.0 + span * shine;
        let d = Vec2::new(0.55 * r, 1.6 * r);
        for (half, a) in [(0.16 * r, 18u8), (0.08 * r, 26), (0.03 * r, 34)] {
            let (l, rr) = (x - half, x + half);
            let mut band = clip(&outline, Pos2::new(l + d.x, c.y + d.y), Pos2::new(l - d.x, c.y - d.y));
            band = clip(&band, Pos2::new(rr - d.x, c.y - d.y), Pos2::new(rr + d.x, c.y + d.y));
            if band.len() >= 3 {
                p.add(Shape::convex_polygon(band, Color32::from_white_alpha(a), Stroke::NONE));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outline_closes_and_facets_meet_at_the_tips() {
        let (apex, chains, tips) = geometry(Pos2::new(100.0, 100.0), 50.0);
        for k in 0..3 {
            let j = (k + 1) % 3;
            assert!((chains[k][0] - tips[k]).length() < 0.01);
            assert!((*chains[k].last().unwrap() - tips[j]).length() < 0.01);
        }
        // Le sommet est à l'intérieur, la forme tient dans le cercle et paraît centrée.
        let all: Vec<Pos2> = chains.iter().flatten().copied().collect();
        let (top, bottom) = all
            .iter()
            .fold((f32::MAX, f32::MIN), |(t, b), p| (t.min(p.y), b.max(p.y)));
        assert!(apex.y > top && apex.y < bottom);
        assert!(((top + bottom) / 2.0 - 100.0).abs() < 3.0, "{top} {bottom}");
    }
}
