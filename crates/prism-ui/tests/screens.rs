//! Rendu hors écran de chaque page (backend simulé) : vérifie que l'interface se
//! dessine sans panique et produit des captures dans `target/ui-shots/`.

use eframe::egui;
use egui_kittest::Harness;
use prism_ui::app::{Page, PrismApp};
use prism_ui::mock::MockBackend;

fn shot(name: &str, page: Page, console: bool) -> image::RgbaImage {
    shot_with(name, page, console, MockBackend::default())
}

fn shot_with(name: &str, page: Page, console: bool, backend: MockBackend) -> image::RgbaImage {
    shot_sized(name, page, console, backend, 820.0)
}

fn shot_sized(name: &str, page: Page, console: bool, backend: MockBackend, height: f32) -> image::RgbaImage {
    let mut app = PrismApp::new(Box::new(backend));
    app.page = page;
    app.console = console;
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1280.0, height))
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

/// Rien ne déborde dans la marge droite de la page (régression vue sur capture :
/// une colonne trop large poussait toute la page hors de la fenêtre).
fn right_margin_is_clear(img: &image::RgbaImage) -> bool {
    let bg = [0x0d, 0x11, 0x17];
    let x = img.width() - 18;
    (120..img.height() - 20)
        .filter(|y| img.get_pixel(x, *y).0[..3] != bg)
        .count()
        < 20
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
        ("services", Page::Services, false),
        ("apparence", Page::Appearance, false),
        ("vie-privee", Page::Privacy, false),
        ("outils", Page::Tools, false),
    ] {
        let img = shot(name, page, console);
        assert_eq!((img.width(), img.height()), (1280, 820), "{name}");
        assert!(has_accent(&img), "{name} : page vide ou thème absent");
        if !console {
            assert!(right_margin_is_clear(&img), "{name} : du contenu déborde à droite");
        }
    }
}

#[test]
fn bar_preview_on_the_left_floating_and_rounded() {
    let mut b = MockBackend::default();
    b.bar.edge = prism_core::bar::Edge::Left;
    b.bar.margin = 12;
    b.bar.rounded = true;
    b.bar.thickness = 56;
    b.bar.opacity = 70;
    let img = shot_with("apparence-barre-gauche", Page::Appearance, false, b);
    assert!(has_accent(&img));
    assert!(right_margin_is_clear(&img));
}

/// Page Apparence entière (fenêtre très haute) : la carte des effets, avec un choix
/// par action, tient dans la largeur.
#[test]
fn appearance_page_with_effects_fits() {
    let mut b = MockBackend::default();
    b.bar.fx.enabled = true;
    let img = shot_sized("apparence-complete", Page::Appearance, false, b, 2000.0);
    assert!(has_accent(&img));
    assert!(right_margin_is_clear(&img), "la carte des effets déborde à droite");
}

/// Thème clair : la page Apparence (thèmes) se dessine avec la palette claire.
#[test]
fn light_theme_renders() {
    let mut b = MockBackend::default();
    b.bar.theme.preset = "clair".into();
    let img = shot_with("apparence-theme-clair", Page::Appearance, false, b);
    // Le fond clair domine la capture.
    let light = img.pixels().filter(|p| p[0] > 200 && p[1] > 200 && p[2] > 180).count();
    assert!(light > (img.width() * img.height()) as usize / 3, "fond clair attendu");
}

/// Page Allègement entière : niveau Extrême et plan « jeu noyau » tiennent dans la largeur.
#[test]
fn allege_page_with_extreme_and_kernel_plan_fits() {
    let img = shot_sized(
        "allegement-complete",
        Page::Allege,
        false,
        MockBackend::default(),
        4200.0,
    );
    assert!(has_accent(&img));
    assert!(right_margin_is_clear(&img), "la page Allègement déborde à droite");
}
