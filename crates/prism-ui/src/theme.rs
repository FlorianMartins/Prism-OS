//! Identité visuelle : sombre, sobre, un seul accent (le cyan de Prism).

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle, Visuals};

pub const BG: Color32 = Color32::from_rgb(0x0d, 0x11, 0x17);
pub const PANEL: Color32 = Color32::from_rgb(0x15, 0x1b, 0x23);
pub const CARD: Color32 = Color32::from_rgb(0x1b, 0x22, 0x2c);
pub const CARD_HI: Color32 = Color32::from_rgb(0x22, 0x2b, 0x37);
pub const BORDER: Color32 = Color32::from_rgb(0x2a, 0x33, 0x40);
pub const TEXT: Color32 = Color32::from_rgb(0xe6, 0xed, 0xf3);
pub const MUTED: Color32 = Color32::from_rgb(0x8b, 0x96, 0xa3);
pub const ACCENT: Color32 = Color32::from_rgb(0x5c, 0xcf, 0xe6);
pub const ACCENT_DIM: Color32 = Color32::from_rgb(0x1d, 0x3d, 0x47);
pub const OK: Color32 = Color32::from_rgb(0x57, 0xd9, 0xa3);
pub const WARN: Color32 = Color32::from_rgb(0xe8, 0xb3, 0x4b);
pub const BAD: Color32 = Color32::from_rgb(0xf4, 0x70, 0x67);

pub fn apply(ctx: &egui::Context) {
    let mut v = Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = PANEL;
    v.extreme_bg_color = Color32::from_rgb(0x0a, 0x0d, 0x12);
    v.faint_bg_color = CARD;
    v.override_text_color = Some(TEXT);
    v.selection.bg_fill = ACCENT_DIM;
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.hyperlink_color = ACCENT;
    let r = CornerRadius::same(6);
    v.widgets.noninteractive.bg_fill = CARD;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    v.widgets.inactive.bg_fill = CARD_HI;
    v.widgets.inactive.weak_bg_fill = CARD_HI;
    v.widgets.inactive.corner_radius = r;
    v.widgets.hovered.bg_fill = Color32::from_rgb(0x2b, 0x36, 0x44);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x2b, 0x36, 0x44);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    v.widgets.hovered.corner_radius = r;
    v.widgets.active.bg_fill = ACCENT_DIM;
    v.widgets.active.weak_bg_fill = ACCENT_DIM;
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
