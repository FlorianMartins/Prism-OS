// Pas de fenêtre console derrière l'interface sous Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]

use prism_ui::app::PrismApp;

fn main() -> eframe::Result<()> {
    #[cfg(windows)]
    let backend: Box<dyn prism_ui::backend::Backend> = Box::new(prism_ui::winbackend::WinBackend::new());
    #[cfg(not(windows))]
    let backend: Box<dyn prism_ui::backend::Backend> = Box::new(prism_ui::mock::MockBackend::default());

    let options = eframe::NativeOptions {
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
            Ok(Box::new(PrismApp::new(backend)))
        }),
    )
}
