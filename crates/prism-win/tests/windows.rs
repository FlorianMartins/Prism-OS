//! Tests d'intégration sur un vrai Windows (CI `windows-latest`).
//!
//! Chaque levier est appliqué à des processus enfants lancés par le test, relu par
//! l'API, restauré, puis relu encore. Les processus de la machine ne sont jamais visés :
//! le relevé est filtré sur nos enfants avant de planifier.
#![cfg(windows)]

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use prism_core::config::Config;
use prism_core::engine::{engage, restore};
use prism_core::journal::MemStore;
use prism_core::model::{EcoState, MemPriority, Priority, ProcId, PurgeScope, Target};
use prism_core::plan::Action;
use prism_core::platform::{Outcome, Platform};
use prism_win::{active_power_plan, proc_id, process_state, WindowsPlatform};

struct Kid(Child);

impl Drop for Kid {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Les enfants héritent des priorités basses de leur parent, et le runner de la CI
/// lance les jobs en `BelowNormal` avec une priorité mémoire basse. On part donc
/// d'un parent remis en `Normal`, sinon les processus de test seraient déjà « en
/// retrait » (et Prism, à raison, refuserait de les remonter).
fn normalize_test_process() {
    use std::ffi::c_void;
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, ProcessMemoryPriority, SetPriorityClass, SetProcessInformation, MEMORY_PRIORITY_INFORMATION,
        NORMAL_PRIORITY_CLASS,
    };
    let info = MEMORY_PRIORITY_INFORMATION { MemoryPriority: 5 };
    // SAFETY: pseudo-handle du processus courant, structure locale de taille exacte.
    unsafe {
        SetPriorityClass(GetCurrentProcess(), NORMAL_PRIORITY_CLASS);
        SetProcessInformation(
            GetCurrentProcess(),
            ProcessMemoryPriority,
            &info as *const _ as *const c_void,
            std::mem::size_of::<MEMORY_PRIORITY_INFORMATION>() as u32,
        );
    }
}

fn spawn(exe: &PathBuf) -> (Kid, Target) {
    normalize_test_process();
    let child = Command::new(exe)
        .args(["-n", "300", "127.0.0.1"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("lancement du processus de test");
    let pid = child.id();
    let id = proc_id(pid).expect("identité du processus de test");
    let name = exe.file_name().unwrap().to_string_lossy().to_lowercase();
    (Kid(child), Target { id, name })
}

fn ping() -> PathBuf {
    PathBuf::from(std::env::var("SystemRoot").unwrap_or("C:\\Windows".into())).join("System32\\ping.exe")
}

/// Copie de ping.exe sous un autre nom, pour jouer le rôle du jeu.
fn fake_game() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("prism-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let exe = dir.join("fakegame.exe");
    if !exe.exists() {
        std::fs::copy(ping(), &exe).unwrap();
    }
    exe
}

fn done_undo(o: Outcome) -> prism_core::journal::Undo {
    match o {
        Outcome::Done(Some(u)) => u,
        other => panic!("attendu Done(Some(undo)), obtenu {other:?}"),
    }
}

#[test]
fn snapshot_sees_our_child_with_its_identity_and_session() {
    let mut w = WindowsPlatform::new();
    let (_kid, t) = spawn(&ping());
    let snap = w.snapshot().unwrap();
    let p = snap.procs.iter().find(|p| p.id == t.id).expect("enfant visible");
    assert_eq!(p.name, "ping.exe");
    assert_eq!(p.session, snap.user_session);
    assert!(p.path.as_deref().unwrap().ends_with("\\system32\\ping.exe"));
    assert!(snap.mem.total > 0 && snap.mem.free <= snap.mem.total);
}

#[test]
fn priority_is_applied_read_back_and_restored() {
    let mut w = WindowsPlatform::new();
    let (_kid, t) = spawn(&ping());
    assert_eq!(process_state(&t).unwrap().0, Priority::Normal);
    let undo = done_undo(w.apply(&Action::Priority {
        target: t.clone(),
        to: Priority::BelowNormal,
    }));
    assert_eq!(process_state(&t).unwrap().0, Priority::BelowNormal);
    // Déjà plus bas : jamais remonté.
    assert!(matches!(
        w.apply(&Action::Priority {
            target: t.clone(),
            to: Priority::BelowNormal
        }),
        Outcome::Skipped(_)
    ));
    assert_eq!(w.undo(&undo), Outcome::Done(None));
    assert_eq!(process_state(&t).unwrap().0, Priority::Normal);
}

#[test]
fn ecoqos_and_memory_priority_are_applied_and_restored() {
    let mut w = WindowsPlatform::new();
    let (_kid, t) = spawn(&ping());
    let (_, eco0, mem0) = process_state(&t).unwrap();
    assert_eq!(mem0, MemPriority::Normal);

    let eco_undo = done_undo(w.apply(&Action::EcoQos { target: t.clone() }));
    let mem_undo = done_undo(w.apply(&Action::MemoryPriority {
        target: t.clone(),
        to: MemPriority::Low,
    }));
    let (_, eco1, mem1) = process_state(&t).unwrap();
    assert_eq!(eco1, EcoState::On);
    assert_eq!(mem1, MemPriority::Low);

    assert_eq!(w.undo(&mem_undo), Outcome::Done(None));
    assert_eq!(w.undo(&eco_undo), Outcome::Done(None));
    let (_, eco2, mem2) = process_state(&t).unwrap();
    assert_eq!((eco2, mem2), (eco0, mem0));
}

#[test]
fn trim_succeeds_on_our_child() {
    let mut w = WindowsPlatform::new();
    let (_kid, t) = spawn(&ping());
    assert_eq!(w.apply(&Action::TrimWorkingSet { target: t }), Outcome::Done(None));
}

#[test]
fn a_wrong_creation_time_is_refused() {
    let mut w = WindowsPlatform::new();
    let (_kid, t) = spawn(&ping());
    let impostor = Target {
        id: ProcId {
            pid: t.id.pid,
            created: t.id.created + 1,
        },
        name: t.name.clone(),
    };
    let o = w.apply(&Action::Priority {
        target: impostor,
        to: Priority::Idle,
    });
    assert!(
        matches!(o, Outcome::Skipped(ref why) if why.contains("réutilisé")),
        "{o:?}"
    );
    assert_eq!(
        process_state(&t).unwrap().0,
        Priority::Normal,
        "le vrai processus n'a pas bougé"
    );
}

#[test]
fn a_dead_process_is_skipped() {
    let mut w = WindowsPlatform::new();
    let (kid, t) = spawn(&ping());
    drop(kid);
    let o = w.apply(&Action::Priority {
        target: t,
        to: Priority::Idle,
    });
    assert!(matches!(o, Outcome::Skipped(_)), "{o:?}");
}

#[test]
fn purge_low_priority_standby_never_fails() {
    let mut w = WindowsPlatform::new();
    let o = w.apply(&Action::PurgeStandby { scope: PurgeScope::Low });
    assert!(!matches!(o, Outcome::Failed(_)), "{o:?}");
    if w.can_purge {
        assert_eq!(o, Outcome::Done(None), "administrateur : la purge doit passer");
    }
}

#[test]
fn power_plan_is_switched_and_restored() {
    let mut w = WindowsPlatform::new();
    let before = active_power_plan().expect("plan actif lisible");
    match w.apply(&Action::HighPerformancePower) {
        Outcome::Done(Some(undo)) => {
            assert_eq!(active_power_plan().unwrap(), "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c");
            assert_eq!(w.undo(&undo), Outcome::Done(None));
            assert_eq!(active_power_plan().unwrap(), before);
        }
        Outcome::Skipped(why) => eprintln!("plan non changé sur cette machine : {why}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn full_game_session_on_real_processes() {
    let mut cfg = Config::builtin();
    cfg.lists.games = vec!["fakegame.exe".into()];
    let mut profile = cfg.profile("gaming").unwrap().clone();
    // Pas d'effet machine dans ce test : on garde les leviers par processus.
    profile.power_plan = prism_core::model::PowerPlan::Unchanged;
    profile.shutdown_wsl = false;
    profile.purge_standby = PurgeScope::Off;

    let mut w = WindowsPlatform::new();
    let (_g, game) = spawn(&fake_game());
    let (_b, bg) = spawn(&ping());

    let mut snap = w.snapshot().unwrap();
    snap.procs.retain(|p| p.id == game.id || p.id == bg.id);
    assert_eq!(
        prism_core::classify::games_running(&snap, &cfg),
        vec!["fakegame.exe".to_string()]
    );

    let mut store = MemStore::default();
    let mut session = None;
    let report = engage(&mut w, &mut store, &cfg, "gaming", &profile, &mut session, &snap);
    assert!(report.failed.is_empty(), "{:?}", report.failed);

    let (gp, ge, gm) = process_state(&game).unwrap();
    assert_eq!(
        (gp, gm),
        (Priority::Normal, MemPriority::Normal),
        "le jeu n'est pas touché"
    );
    assert_ne!(ge, EcoState::On, "le jeu n'est pas mis en EcoQoS");
    let (p, e, m) = process_state(&bg).unwrap();
    assert_eq!((p, e, m), (Priority::BelowNormal, EcoState::On, MemPriority::Low));

    let report = restore(&mut w, &mut store, &session.unwrap().journal);
    assert!(report.failed.is_empty() && report.skipped.is_empty(), "{report:?}");
    let (p, _, m) = process_state(&bg).unwrap();
    assert_eq!((p, m), (Priority::Normal, MemPriority::Normal));
}
