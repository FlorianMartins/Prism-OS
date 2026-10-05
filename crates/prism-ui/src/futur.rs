//! Briques visuelles « futuristes » de l'appli : carte interactive en relief (elle
//! s'incline vers la souris, reflet qui suit le curseur, ombre décalée, bordure
//! lumineuse cyan → violet) et jauge circulaire. Les animations ne tournent que pendant
//! le survol ou une transition : au repos, rien ne redessine.

use eframe::egui::{self, Color32, CornerRadius, Margin, Pos2, Rect, Shape, Stroke, Vec2};

use crate::theme as th;

const CYAN: Color32 = Color32::from_rgb(34, 211, 238);
const VIOLET: Color32 = Color32::from_rgb(167, 139, 250);

fn lerp_color(a: Color32, b: Color32, k: f32) -> Color32 {
    let k = k.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * k) as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

/// Carte en relief. `highlighted` : carte active (profil choisi…).
pub fn card3d<R>(ui: &mut egui::Ui, highlighted: bool, add: impl FnOnce(&mut egui::Ui) -> R) -> egui::Response {
    // Apparition en cascade au changement de page.
    let a = appear(ui);
    if a < 1.0 {
        ui.add_space((1.0 - a) * 10.0);
    }
    ui.scope(|ui| {
        ui.set_opacity(a);
        card3d_inner(ui, highlighted, add)
    })
    .inner
}

fn card3d_inner<R>(ui: &mut egui::Ui, highlighted: bool, add: impl FnOnce(&mut egui::Ui) -> R) -> egui::Response {
    let id = ui.next_auto_id().with("card3d");
    let ctx = ui.ctx().clone();
    // Inclinaison de l'image précédente (de −1 à 1 sur chaque axe), lissée.
    let target: Vec2 = ctx.data(|d| d.get_temp(id)).unwrap_or(Vec2::ZERO);
    let tx = ctx.animate_value_with_time(id.with("x"), target.x, 0.14);
    let ty = ctx.animate_value_with_time(id.with("y"), target.y, 0.14);
    let hover = ctx.animate_bool_with_time(id.with("h"), target != Vec2::ZERO, 0.16);
    // Le contenu glisse un peu dans le sens de l'inclinaison (parallaxe).
    let px = (tx * 3.0).round() as i8;
    let py = (ty * 3.0).round() as i8;
    let bg_idx = ui.painter().add(Shape::Noop);
    let fill = if highlighted { th::accent_dim() } else { th::card() };
    let inner = egui::Frame::new()
        .inner_margin(Margin {
            left: (14 + px).max(8),
            right: (14 - px).max(8),
            top: (14 + py).max(8),
            bottom: (14 - py).max(8),
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        });
    let rect = inner.response.rect;
    let resp = ui.interact(rect, id.with("hit"), egui::Sense::hover());
    // Nouvelle inclinaison selon la souris.
    let tilt = match resp.hover_pos() {
        Some(p) if resp.hovered() => {
            let c = rect.center();
            Vec2::new(
                ((p.x - c.x) / (rect.width() / 2.0)).clamp(-1.0, 1.0),
                ((p.y - c.y) / (rect.height() / 2.0)).clamp(-1.0, 1.0),
            )
        }
        _ => Vec2::ZERO,
    };
    if tilt != target {
        ctx.data_mut(|d| d.insert_temp(id, tilt));
        ctx.request_repaint();
    }

    let r = 12.0;
    let mut shapes = Vec::new();
    // Ombre portée, à l'opposé de l'inclinaison.
    let shadow_off = Vec2::new(-tx * 6.0, -ty * 6.0 + 4.0 * hover);
    for (i, a) in [(8.0, 18u8), (4.0, 28)] {
        shapes.push(Shape::rect_filled(
            rect.translate(shadow_off).expand(i * hover),
            CornerRadius::same((r + i) as u8),
            Color32::from_black_alpha((a as f32 * hover) as u8),
        ));
    }
    // Fond : léger dégradé vertical (haut plus clair), bord du côté levé éclairé.
    shapes.push(Shape::rect_filled(rect, CornerRadius::same(r as u8), fill));
    let top_band = Rect::from_min_max(rect.min, Pos2::new(rect.max.x, rect.min.y + rect.height() * 0.45));
    shapes.push(Shape::rect_filled(
        top_band,
        CornerRadius {
            nw: r as u8,
            ne: r as u8,
            sw: 0,
            se: 0,
        },
        Color32::from_white_alpha(6),
    ));
    // Bordure : discrète au repos, lumineuse cyan → violet au survol ou si active.
    let glow = if highlighted { 1.0 } else { hover };
    let border = lerp_color(th::border(), lerp_color(CYAN, VIOLET, 0.5 + tx * 0.5), glow);
    if glow > 0.01 {
        shapes.push(Shape::rect_stroke(
            rect.expand(2.0),
            CornerRadius::same((r + 2.0) as u8),
            Stroke::new(3.0, border.gamma_multiply(0.18 * glow)),
            egui::StrokeKind::Middle,
        ));
    }
    shapes.push(Shape::rect_stroke(
        rect,
        CornerRadius::same(r as u8),
        Stroke::new(1.0, border),
        egui::StrokeKind::Inside,
    ));
    ui.painter().set(bg_idx, Shape::Vec(shapes));
    // Reflet qui suit la souris (au-dessus du fond, sous le texte n'est pas possible
    // sans le cacher : très transparent).
    if hover > 0.01 {
        if let Some(p) = resp.hover_pos() {
            let painter = ui.painter().with_clip_rect(rect.shrink(1.0));
            // Dégradé radial lisse (un seul maillage) : centre teinté cyan, bord transparent.
            let big = rect.width().max(rect.height()) * 0.6;
            let mut mesh = egui::epaint::Mesh::default();
            let centre = Color32::from_rgba_unmultiplied(120, 220, 255, (18.0 * hover) as u8);
            mesh.colored_vertex(p, centre);
            let n = 48u32;
            for k in 0..=n {
                let a = k as f32 / n as f32 * std::f32::consts::TAU;
                mesh.colored_vertex(p + Vec2::angled(a) * big, Color32::TRANSPARENT);
            }
            for k in 1..=n {
                mesh.add_triangle(0, k, k + 1);
            }
            painter.add(Shape::mesh(mesh));
        }
    }
    resp
}

/// Jauge circulaire (0 à 100 %), arc dégradé cyan → violet, valeur au centre.
pub fn ring(ui: &mut egui::Ui, value: Option<f32>, label: &str, detail: &str) {
    let size = 112.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), size + 22.0), egui::Sense::hover());
    let c = Pos2::new(rect.center().x, rect.min.y + size / 2.0);
    let radius = size * 0.40;
    let p = ui.painter();
    // Piste.
    p.circle_stroke(c, radius, Stroke::new(8.0, th::card_hi()));
    // Valeur animée (glisse vers la nouvelle mesure).
    let id = ui.id().with(("ring", label));
    let v = ui
        .ctx()
        .animate_value_with_time(id, value.unwrap_or(0.0).clamp(0.0, 100.0), 0.6);
    let start = -std::f32::consts::FRAC_PI_2;
    let n = 72;
    let filled = (v / 100.0 * n as f32).round() as usize;
    for i in 0..filled {
        let a0 = start + i as f32 / n as f32 * std::f32::consts::TAU;
        let a1 = start + (i + 1) as f32 / n as f32 * std::f32::consts::TAU;
        let col = lerp_color(CYAN, VIOLET, i as f32 / n as f32);
        p.line_segment(
            [c + Vec2::angled(a0) * radius, c + Vec2::angled(a1) * radius],
            Stroke::new(8.0, col),
        );
    }
    if filled > 0 {
        // Extrémité lumineuse.
        let a = start + filled as f32 / n as f32 * std::f32::consts::TAU;
        let tip = c + Vec2::angled(a) * radius;
        p.circle_filled(
            tip,
            6.0,
            lerp_color(CYAN, VIOLET, filled as f32 / n as f32).gamma_multiply(0.35),
        );
    }
    let text = match value {
        Some(_) => format!("{v:.0}%"),
        None => "—".into(),
    };
    p.text(
        c,
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(26.0),
        th::text(),
    );
    p.text(
        Pos2::new(c.x, c.y + radius + 18.0),
        egui::Align2::CENTER_CENTER,
        label.to_uppercase(),
        egui::FontId::proportional(12.0),
        th::accent(),
    );
    if !detail.is_empty() {
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new(detail).small().color(th::muted()));
        });
    }
}

/// Fond « technologique » derrière les pages : dégradé, grille fine, deux halos doux,
/// vignette. Statique : ne demande aucun redessin.
pub fn backdrop(painter: &egui::Painter, rect: Rect) {
    let bg = th::bg();
    painter.rect_filled(rect, 0.0, bg);
    // Grille fine (accent très transparent), plus nette vers le haut.
    let step = 44.0;
    let grid = th::accent().gamma_multiply(0.035);
    let mut x = rect.left() + (rect.left() % step);
    while x < rect.right() {
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            Stroke::new(1.0, grid),
        );
        x += step;
    }
    let mut y = rect.top();
    while y < rect.bottom() {
        painter.line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            Stroke::new(1.0, grid),
        );
        y += step;
    }
    // Halos : cyan en haut à droite, violet en bas à gauche.
    glow(
        painter,
        Pos2::new(rect.right() - 80.0, rect.top() + 40.0),
        rect.width() * 0.45,
        CYAN,
        22,
    );
    glow(
        painter,
        Pos2::new(rect.left() + 60.0, rect.bottom() - 30.0),
        rect.width() * 0.40,
        VIOLET,
        18,
    );
}

/// Halo radial lisse (un maillage du centre coloré au bord transparent).
pub fn glow(painter: &egui::Painter, c: Pos2, radius: f32, col: Color32, alpha: u8) {
    let mut mesh = egui::epaint::Mesh::default();
    mesh.colored_vertex(c, Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), alpha));
    let n = 48u32;
    for k in 0..=n {
        let a = k as f32 / n as f32 * std::f32::consts::TAU;
        mesh.colored_vertex(c + Vec2::angled(a) * radius, Color32::TRANSPARENT);
    }
    for k in 1..=n {
        mesh.add_triangle(0, k, k + 1);
    }
    painter.add(Shape::mesh(mesh));
}

/// Trait lumineux dégradé cyan → violet, de longueur `k` (0 à 1) : sous les titres.
pub fn underline(painter: &egui::Painter, left: Pos2, width: f32, k: f32) {
    let w = width * k.clamp(0.0, 1.0);
    if w <= 1.0 {
        return;
    }
    let n = 24;
    for i in 0..n {
        let a = left + Vec2::new(w * i as f32 / n as f32, 0.0);
        let b = left + Vec2::new(w * (i + 1) as f32 / n as f32, 0.0);
        let col = lerp_color(CYAN, VIOLET, i as f32 / n as f32);
        painter.line_segment([a, b], Stroke::new(2.0, col));
        painter.line_segment([a, b], Stroke::new(6.0, col.gamma_multiply(0.12)));
    }
}

/// Apparition en cascade des cartes d'une page : chaque carte arrive un peu après la
/// précédente (opacité et léger glissement). L'appli note l'instant du changement de
/// page ; les cartes se numérotent à chaque image.
pub fn appear(ui: &mut egui::Ui) -> f32 {
    let ctx = ui.ctx().clone();
    let (since, now) = (
        ctx.data(|d| d.get_temp::<f64>(egui::Id::new("prism-page-since")))
            .unwrap_or(0.0),
        ctx.input(|i| i.time),
    );
    let idx = ctx.data_mut(|d| {
        let n = d.get_temp_mut_or_default::<u32>(egui::Id::new("prism-card-idx"));
        *n += 1;
        *n - 1
    });
    let k = ((now - since - idx.min(12) as f64 * 0.035) / 0.28).clamp(0.0, 1.0) as f32;
    if k < 1.0 {
        ctx.request_repaint();
    }
    1.0 - (1.0 - k).powi(3)
}
