//! Captures des trois écrans de l'installateur (simulation, temps figé).

use eframe::egui;
use egui_kittest::Harness;
use prism_setup::{MockInstaller, Screen, SetupApp};

fn shot(name: &str, prepare: impl FnOnce(&mut SetupApp)) -> image::RgbaImage {
    let mut app = SetupApp::new(Box::new(MockInstaller::default()));
    app.frozen_time = Some(5.0);
    prepare(&mut app);
    let mut h = Harness::builder()
        .with_size(egui::vec2(760.0, 560.0))
        .wgpu()
        .build_ui_state(|ui, app: &mut SetupApp| app.show(ui), app);
    h.run_steps(3);
    let img = h.render().expect("rendu");
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-shots");
    std::fs::create_dir_all(&dir).unwrap();
    img.save(dir.join(format!("setup-{name}.png"))).unwrap();
    img
}

/// Pixels nettement colorés (faces du prisme : violet, rose, cyan).
fn colourful(img: &image::RgbaImage) -> usize {
    img.pixels()
        .filter(|p| {
            let (mx, mn) = (p[0].max(p[1]).max(p[2]), p[0].min(p[1]).min(p[2]));
            mx > 120 && mx - mn > 70
        })
        .count()
}

#[test]
fn every_screen_renders_with_the_prism_logo() {
    let w = shot("bienvenue", |_| {});
    assert!(colourful(&w) > 200, "le logo (faces colorées) est dessiné");
    let i = shot("installation", |a| {
        a.screen = Screen::Installing;
        if let Ok(mut p) = a.progress().lock() {
            p.fraction = 0.55;
            p.step = 1;
        }
    });
    assert!(colourful(&i) > 200);
    let f = shot("fin", |a| a.screen = Screen::Finished);
    assert!(colourful(&f) > 200);
}
