//! `prism demo` : une partie simulée de bout en bout, sur n'importe quel système.

use prism_core::config::Config;
use prism_core::journal::MemStore;
use prism_core::mock::MockPlatform;
use prism_core::model::human_bytes;
use prism_core::platform::Platform;
use prism_core::watch::{Event, Watcher};

use crate::render;

const GIB: u64 = 1024 * 1024 * 1024;

pub fn run(cfg: &Config) -> Result<(), String> {
    let profile = cfg.profile("gaming")?;
    let mut m = MockPlatform::new(16 * GIB);
    m.spawn("WINWORD.EXE", None, 2 * GIB);
    m.spawn("chrome.exe", None, 3 * GIB);
    m.spawn("Teams.exe", None, GIB);
    m.spawn("Discord.exe", None, GIB / 2);
    m.spawn("vgc.exe", None, GIB / 10);
    m.standby[5] = 4 * GIB;
    m.wsl_running = true;

    println!("Simulation — PC 16 Go après une longue session de bureautique.");
    println!("Word, Chrome et Teams sont encore ouverts ; WSL tourne ; Discord et Vanguard aussi.\n");
    let game = m.spawn(
        "VALORANT-Win64-Shipping.exe",
        Some(r"C:\Riot Games\VALORANT\live\ShooterGame\Binaries\Win64\VALORANT-Win64-Shipping.exe"),
        4 * GIB,
    );
    println!("> Lancement de VALORANT");
    println!("{}\n", render::memory(&m.mem()));

    let mut store = MemStore::default();
    let mut w = Watcher::default();
    let mut step = |m: &mut MockPlatform, w: &mut Watcher| {
        let snap = m.snapshot().expect("simulation");
        w.tick(m, &mut store, cfg, "gaming", profile, &snap)
    };

    if let Event::Engaged {
        games,
        report,
        conflicts,
    } = step(&mut m, &mut w)
    {
        println!("Mode Jeu activé pour {} :", games.join(", "));
        render::report(&report, "  ");
        render::conflicts(&conflicts);
    }
    println!("\n{}", render::memory(&m.mem()));
    println!("  (le cache normal de 4 Go, où vivent les données du jeu, est intact)\n");

    m.kill(game);
    println!("> Fermeture de VALORANT");
    loop {
        match step(&mut m, &mut w) {
            Event::Cooling { remaining } => println!("  attente avant restauration ({remaining} passage(s))"),
            Event::Released { report } => {
                println!("Mode Jeu terminé :");
                render::restored(&report, "  ");
                break;
            }
            other => return Err(format!("simulation : événement inattendu {other:?}")),
        }
    }
    let word = m.procs.iter().find(|p| p.info.name == "winword.exe").expect("word");
    println!(
        "\nWord : priorité {:?}, priorité mémoire {:?} — comme avant la partie. Sa RAM ({}) reviendra quand il servira.",
        word.priority,
        word.mem_priority,
        human_bytes(2 * GIB)
    );
    Ok(())
}
