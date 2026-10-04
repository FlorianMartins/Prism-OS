//! Prism Bar : barre des tâches personnalisable (bord, épaisseur, opacité, widgets).
#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
fn main() {
    if std::env::args().any(|a| a == "--stop") {
        prism_win::bar_app::stop();
        return;
    }
    if let Err(e) = prism_win::bar_app::run() {
        eprintln!("prism-bar : {e}");
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("prism-bar ne fonctionne que sous Windows");
}
