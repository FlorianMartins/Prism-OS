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
            .with_min_inner_size([960.0, 640.0])
            // Icône losange bleu-violet (barre de titre, barre des tâches, Alt+Tab).
            .with_icon(std::sync::Arc::new(eframe::egui::IconData {
                rgba: include_bytes!("../assets/prism-64.rgba").to_vec(),
                width: 64,
                height: 64,
            })),
        ..Default::default()
    };
    eframe::run_native(
        "Prism",
        options,
        Box::new(|cc| {
            prism_ui::theme::apply(&cc.egui_ctx);
            Ok(Box::new(PrismApp::new(backend()).with_factory(backend)))
        }),
    )
}

/// wgpu (DirectX 12 / Vulkan) d'abord, OpenGL ensuite : une carte graphique ou une
/// session qui refuse l'un n'empêche pas l'appli de s'ouvrir. Si rien ne marche,
/// l'erreur est montrée (sans console, l'appli se fermait sans rien dire).
fn main() {
    // Sans droits administrateur, l'appli se relance par la tâche « Prism (admin) » posée
    // à l'installation : droits obtenus sans fenêtre de confirmation. Lancée par cette
    // tâche (`--depuis-tache`), elle ne recommence jamais (compte standard : mode limité).
    #[cfg(windows)]
    if !prism_win::is_elevated() && !std::env::args().any(|a| a == "--depuis-tache") {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let ok = std::process::Command::new("schtasks.exe")
            .args(["/Run", "/TN", "Prism (admin)"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if ok {
            return;
        }
    }
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
