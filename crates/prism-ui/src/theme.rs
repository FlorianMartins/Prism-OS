//! Identité visuelle : un fond, un seul accent ; palette au choix (thèmes de
//! `prism_core::theme`, cyan de Prism par défaut).

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle, Visuals};
use prism_core::theme::{mix, Palette, Rgb, ThemeConfig};

thread_local! {
    /// Palette active (thème choisi dans Apparence, `bar.json`). Par fil : l'interface
    /// est dessinée sur un seul fil, et les tests de captures, parallèles, ne se
    /// marchent pas dessus.
    static PALETTE: std::cell::Cell<Option<(Palette, Rgb)>> = const { std::cell::Cell::new(None) };
}

fn current() -> (Palette, Rgb) {
    PALETTE.with(|p| p.get()).unwrap_or_else(|| {
        let t = ThemeConfig::default();
        (t.palette(), t.on_accent())
    })
}

fn c(rgb: Rgb) -> Color32 {
    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

pub fn bg() -> Color32 {
    c(current().0.bg)
}
pub fn panel() -> Color32 {
    c(current().0.panel)
}
pub fn card() -> Color32 {
    c(current().0.card)
}
pub fn card_hi() -> Color32 {
    c(current().0.card_hi)
}
pub fn border() -> Color32 {
    c(current().0.border)
}
pub fn text() -> Color32 {
    c(current().0.text)
}
pub fn muted() -> Color32 {
    c(current().0.muted)
}
pub fn accent() -> Color32 {
    c(current().0.accent)
}
pub fn accent_dim() -> Color32 {
    c(current().0.accent_dim)
}
/// Texte posé sur l'accent (boutons pleins).
pub fn on_accent() -> Color32 {
    c(current().1)
}
pub fn ok() -> Color32 {
    c(current().0.ok)
}
pub fn warn() -> Color32 {
    c(current().0.warn)
}
pub fn bad() -> Color32 {
    c(current().0.bad)
}

/// Applique un thème (au démarrage et quand il change dans Apparence).
pub fn set_theme(ctx: &egui::Context, theme: &ThemeConfig) {
    PALETTE.with(|p| p.set(Some((theme.palette(), theme.on_accent()))));
    apply(ctx);
}

pub fn apply(ctx: &egui::Context) {
    let (p, _) = current();
    let dark = prism_core::theme::luminance(p.bg) < 0.4;
    let mut v = if dark { Visuals::dark() } else { Visuals::light() };
    v.panel_fill = bg();
    v.window_fill = panel();
    v.extreme_bg_color = c(mix(p.bg, if dark { [0, 0, 0] } else { [255, 255, 255] }, 0.3));
    v.faint_bg_color = card();
    v.override_text_color = Some(text());
    v.selection.bg_fill = accent_dim();
    v.selection.stroke = Stroke::new(1.0, accent());
    v.hyperlink_color = accent();
    let r = CornerRadius::same(6);
    v.widgets.noninteractive.bg_fill = card();
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, border());
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, text());
    v.widgets.inactive.bg_fill = card_hi();
    v.widgets.inactive.weak_bg_fill = card_hi();
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, text());
    v.widgets.inactive.corner_radius = r;
    let hover = c(mix(p.card_hi, p.text, 0.08));
    v.widgets.hovered.bg_fill = hover;
    v.widgets.hovered.weak_bg_fill = hover;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, accent());
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, text());
    v.widgets.hovered.corner_radius = r;
    v.widgets.active.bg_fill = accent_dim();
    v.widgets.active.weak_bg_fill = accent_dim();
    v.widgets.active.fg_stroke = Stroke::new(1.0, text());
    v.widgets.active.corner_radius = r;
    ctx.set_visuals(v);

    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Heading, FontId::new(22.0, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(14.5, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(14.5, FontFamily::Proportional)),
            (TextStyle::Small, FontId::new(12.0, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(10.0, 8.0);
        style.spacing.button_padding = egui::vec2(14.0, 7.0);
    });
}
