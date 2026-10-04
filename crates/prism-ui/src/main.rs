// Pas de fenêtre console derrière l'interface sous Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]

use prism_ui::app::PrismApp;

fn backend() -> Box<dyn prism_ui::backend::Backend> {
    #[cfg(windows)]
    return Box::new(prism_ui::winbackend::WinBackend::new());
    #[cfg(not(windows))]
    return Box::new(prism_ui::mock::MockBackend::default());
}

fn run(renderer: eframe::Renderer) -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        renderer,
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Prism")
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([960.0, 640.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Prism",
        options,
        Box::new(|cc| {
            prism_ui::theme::apply(&cc.egui_ctx);
            Ok(Box::new(PrismApp::new(backend())))
        }),
    )
}

/// wgpu (DirectX 12 / Vulkan) d'abord, OpenGL ensuite : une carte graphique ou une
/// session qui refuse l'un n'empêche pas l'appli de s'ouvrir. Si rien ne marche,
/// l'erreur est montrée (sans console, l'appli se fermait sans rien dire).
fn main() {
    let Err(first) = run(eframe::Renderer::Wgpu) else {
        return;
    };
    let Err(second) = run(eframe::Renderer::Glow) else {
        return;
    };
    let msg = format!(
        "Prism n'a pas pu s'ouvrir : aucun rendu graphique disponible.\n\nDirectX 12 / Vulkan : {first}\nOpenGL : {second}\n\nLes réglages restent accessibles en ligne de commande : prism status"
    );
    #[cfg(windows)]
    prism_win::install::error_box("Prism", &msg);
    #[cfg(not(windows))]
    eprintln!("{msg}");
    std::process::exit(1);
}
