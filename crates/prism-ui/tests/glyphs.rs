use eframe::egui;
use egui_kittest::Harness;

#[test]
#[ignore = "outil : planche des glyphes disponibles"]
fn glyph_sheet() {
    let candidates = "⌂ ☰ ⚙ ⏻ ⚡ ★ ✔ ● ◆ ◈ ▣ ⬢ 🎮 🏠 🚀 🧹 🛡 ⛭ ⏵ ⏸ ⬛ ■ □ ◼ ▲ ▼ ♦ ✦ ✧ ❖ ⌁ ☁ ♻ ⚠ ⚒ ⛶ 🗲 💾 📊 📈 🔒 🔓 🗑 ⏏ ⟲ ↺ ⊕ ⊗ ⋯ ☑ ☐ ⬤ ◉ ○ ◐ ✚ ✖ ⬆ ⬇ ➜ 🖥 🕹";
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 220.0))
        .wgpu()
        .build_ui(|ui| {
            for chunk in candidates.split(' ').collect::<Vec<_>>().chunks(16) {
                ui.horizontal(|ui| {
                    for g in chunk {
                        ui.label(egui::RichText::new(*g).size(24.0));
                    }
                });
            }
        });
    h.run_steps(2);
    h.render()
        .unwrap()
        .save(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../target/ui-shots/glyphes.png"
        ))
        .unwrap();
}
