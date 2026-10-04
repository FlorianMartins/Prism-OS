//! Rendu hors écran de chaque page (backend simulé) : vérifie que l'interface se
//! dessine sans panique et produit des captures dans `target/ui-shots/`.

use eframe::egui;
use egui_kittest::Harness;
use prism_ui::app::{Page, PrismApp};
use prism_ui::mock::MockBackend;

fn shot(name: &str, page: Page, console: bool) -> image::RgbaImage {
    let mut app = PrismApp::new(Box::new(MockBackend::default()));
    app.page = page;
    app.console = console;
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1280.0, 820.0))
        .wgpu()
        .build_ui_state(|ui, app: &mut PrismApp| app.show(ui), app);
    prism_ui::theme::apply(&harness.ctx);
    harness.run_steps(4);
    let img = harness.render().expect("rendu wgpu");
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-shots");
    std::fs::create_dir_all(&dir).unwrap();
    img.save(dir.join(format!("{name}.png"))).unwrap();
    img
}

/// Une capture n'est pas un écran vide : elle contient l'accent cyan de Prism.
fn has_accent(img: &image::RgbaImage) -> bool {
    img.pixels().filter(|p| p[0] < 120 && p[1] > 170 && p[2] > 190).count() > 50
}

#[test]
fn every_page_renders() {
    for (name, page, console) in [
        ("tableau-de-bord", Page::Dashboard, false),
        ("jeux", Page::Games, false),
        ("mode-console", Page::Games, true),
        ("demarrage", Page::Startup, false),
        ("allegement", Page::Allege, false),
        ("apparence", Page::Appearance, false),
        ("outils", Page::Tools, false),
    ] {
        let img = shot(name, page, console);
        assert_eq!((img.width(), img.height()), (1280, 820), "{name}");
        assert!(has_accent(&img), "{name} : page vide ou thème absent");
    }
}
