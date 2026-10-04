//! Tests du moteur de bout en bout, sur la plateforme simulée.

use prism_core::config::Config;
use prism_core::engine::{clean, engage, journal_touches, recover, restore};
use prism_core::journal::{JournalStore, MemStore};
use prism_core::mock::{MockPlatform, BALANCED_PLAN, HIGH_PERF_PLAN};
use prism_core::model::{EcoState, MemPriority, Priority};
use prism_core::platform::Platform;
use prism_core::watch::{Event, Watcher};

const GIB: u64 = 1024 * 1024 * 1024;
const GAME: &str = r"C:\Program Files (x86)\Steam\steamapps\common\ELDEN RING\Game\eldenring.exe";

/// PC 16 Go après une longue session de bureautique : Word et Chrome encore ouverts,
/// 4 Go de cache de fichiers normal (dont les données du jeu), peu de RAM libre.
fn office_then_game() -> MockPlatform {
    let mut m = MockPlatform::new(16 * GIB);
    m.spawn("WINWORD.EXE", None, 2 * GIB);
    m.spawn("chrome.exe", None, 3 * GIB);
    m.spawn("Discord.exe", None, GIB / 2);
    m.spawn("vgc.exe", None, GIB / 10);
    m.standby[5] = 4 * GIB;
    m.spawn("eldenring.exe", Some(GAME), 4 * GIB);
    m.wsl_running = true;
    m
}

fn by_name<'a>(m: &'a MockPlatform, name: &str) -> &'a prism_core::mock::MockProc {
    m.procs.iter().find(|p| p.info.name == name).unwrap()
}

#[test]
fn word_left_open_gives_its_ram_to_the_game_without_touching_the_game_cache() {
    let cfg = Config::builtin();
    let profile = cfg.profile("gaming").unwrap();
    let mut m = office_then_game();
    let before = m.mem();
    assert!(
        before.free_percent() < 25,
        "scénario de départ : RAM tendue ({}%)",
        before.free_percent()
    );

    let mut store = MemStore::default();
    let mut session = None;
    let snap = m.snapshot().unwrap();
    let report = engage(&mut m, &mut store, &cfg, "gaming", profile, &mut session, &snap);
    assert!(report.failed.is_empty(), "{:?}", report.failed);

    let after = m.mem();
    // Les 5 Go de Word + Chrome sont réellement libres…
    assert!(
        after.free >= before.free + 5 * GIB,
        "libre avant {} après {}",
        before.free,
        after.free
    );
    // …et le cache normal (données du jeu) est intact.
    assert_eq!(m.standby[5], 4 * GIB);
    // Le jeu, l'anti-cheat et Discord n'ont pas bougé.
    for untouched in ["eldenring.exe", "vgc.exe", "discord.exe"] {
        let p = by_name(&m, untouched);
        assert_eq!(p.priority, Priority::Normal, "{untouched}");
        assert_eq!(p.mem_priority, MemPriority::Normal, "{untouched}");
        assert!(p.info.working_set > 0, "{untouched} ne doit pas être rogné");
    }
    assert_eq!(by_name(&m, "eldenring.exe").info.working_set, 4 * GIB);
    // L'arrière-plan est en retrait.
    let word = by_name(&m, "winword.exe");
    assert_eq!((word.priority, word.eco), (Priority::BelowNormal, EcoState::On));
    assert_eq!(m.power_plan, HIGH_PERF_PLAN);
    assert!(!m.wsl_running, "WSL arrêté pour rendre vmmem");
}

#[test]
fn release_restores_everything_exactly() {
    let cfg = Config::builtin();
    let profile = cfg.profile("gaming").unwrap();
    let mut m = office_then_game();
    // Un processus qui avait déjà une priorité non standard doit la retrouver.
    let chrome = m.procs.iter_mut().find(|p| p.info.name == "chrome.exe").unwrap();
    chrome.priority = Priority::AboveNormal;
    chrome.eco = EcoState::Off;
    chrome.mem_priority = MemPriority::Medium;
    let original: Vec<_> = m
        .procs
        .iter()
        .map(|p| (p.info.id, p.priority, p.eco, p.mem_priority))
        .collect();

    let mut store = MemStore::default();
    let mut session = None;
    let snap = m.snapshot().unwrap();
    engage(&mut m, &mut store, &cfg, "gaming", profile, &mut session, &snap);
    let journal = session.take().unwrap().journal;
    let report = restore(&mut m, &mut store, &journal);
    assert!(report.failed.is_empty() && report.skipped.is_empty(), "{report:?}");

    let now: Vec<_> = m
        .procs
        .iter()
        .map(|p| (p.info.id, p.priority, p.eco, p.mem_priority))
        .collect();
    assert_eq!(now, original);
    assert_eq!(m.power_plan, BALANCED_PLAN);
    assert_eq!(store.load().unwrap(), None, "journal effacé après restauration");
}

#[test]
fn every_reversible_change_is_journaled_before_the_next_one() {
    let cfg = Config::builtin();
    let mut m = office_then_game();
    let mut store = MemStore::default();
    let mut session = None;
    let snap = m.snapshot().unwrap();
    engage(
        &mut m,
        &mut store,
        &cfg,
        "gaming",
        cfg.profile("gaming").unwrap(),
        &mut session,
        &snap,
    );
    let entries = session.unwrap().journal.entries.len();
    assert!(entries > 0);
    assert_eq!(store.saves, entries, "une écriture par changement réversible");
}

#[test]
fn crash_mid_game_is_recovered_on_next_start() {
    let cfg = Config::builtin();
    let mut m = office_then_game();
    let mut store = MemStore::default();
    {
        let mut session = None;
        let snap = m.snapshot().unwrap();
        engage(
            &mut m,
            &mut store,
            &cfg,
            "gaming",
            cfg.profile("gaming").unwrap(),
            &mut session,
            &snap,
        );
        // Prism est tué ici : la session en mémoire est perdue, seul le journal reste.
    }
    assert_eq!(by_name(&m, "winword.exe").priority, Priority::BelowNormal);
    let report = recover(&mut m, &mut store).unwrap().expect("un journal devait traîner");
    assert!(report.failed.is_empty());
    assert_eq!(by_name(&m, "winword.exe").priority, Priority::Normal);
    assert_eq!(m.power_plan, BALANCED_PLAN);
    assert!(
        recover(&mut m, &mut store).unwrap().is_none(),
        "rien à refaire la fois suivante"
    );
}

#[test]
fn a_reused_pid_is_never_restored_by_mistake() {
    let cfg = Config::builtin();
    let mut m = office_then_game();
    let word = by_name(&m, "winword.exe").info.id;
    let mut store = MemStore::default();
    let mut session = None;
    let snap = m.snapshot().unwrap();
    engage(
        &mut m,
        &mut store,
        &cfg,
        "gaming",
        cfg.profile("gaming").unwrap(),
        &mut session,
        &snap,
    );

    // Word se ferme ; un autre programme hérite de son PID et règle sa propre priorité.
    let intruder = m.reuse_pid(word, "autre.exe");
    m.procs.iter_mut().find(|p| p.info.id == intruder).unwrap().priority = Priority::High;

    let report = restore(&mut m, &mut store, &session.unwrap().journal);
    assert_eq!(
        m.get(intruder).unwrap().priority,
        Priority::High,
        "le nouveau processus n'est pas touché"
    );
    assert!(report
        .skipped
        .iter()
        .any(|s| s.contains("winword.exe") && s.contains("disparu")));
}

#[test]
fn a_process_we_cannot_open_is_skipped_and_not_journaled() {
    let cfg = Config::builtin();
    let mut m = office_then_game();
    let chrome = m.procs.iter_mut().find(|p| p.info.name == "chrome.exe").unwrap();
    chrome.denied = true;
    let chrome_id = chrome.info.id;
    let mut store = MemStore::default();
    let mut session = None;
    let snap = m.snapshot().unwrap();
    let report = engage(
        &mut m,
        &mut store,
        &cfg,
        "gaming",
        cfg.profile("gaming").unwrap(),
        &mut session,
        &snap,
    );
    assert!(report.failed.is_empty());
    assert!(report.skipped.iter().any(|s| s.contains("chrome.exe")));
    assert!(!journal_touches(&session.unwrap().journal, chrome_id));
}

#[test]
fn watcher_engages_follows_new_processes_waits_then_releases() {
    let mut cfg = Config::builtin();
    cfg.release_after_polls = 3;
    let profile = cfg.profile("gaming").unwrap().clone();
    let mut m = office_then_game();
    let game = by_name(&m, "eldenring.exe").info.id;
    let mut store = MemStore::default();
    let mut w = Watcher::default();
    let tick = |m: &mut MockPlatform, w: &mut Watcher, store: &mut MemStore| {
        let snap = m.snapshot().unwrap();
        w.tick(m, store, &cfg, "gaming", &profile, &snap)
    };

    assert!(
        matches!(tick(&mut m, &mut w, &mut store), Event::Engaged { ref games, .. } if games == &["eldenring.exe".to_string()])
    );
    assert_eq!(tick(&mut m, &mut w, &mut store), Event::Idle, "rien de neuf");

    // Un programme lancé en pleine partie est mis en retrait au passage suivant.
    let late = m.spawn("teams.exe", None, GIB);
    assert!(matches!(tick(&mut m, &mut w, &mut store), Event::Updated { .. }));
    assert_eq!(m.get(late).unwrap().priority, Priority::BelowNormal);
    assert_eq!(
        m.get(late).unwrap().info.working_set,
        GIB,
        "pas de rognage après le 1er passage"
    );

    // Le jeu se ferme : on attend avant de restaurer (relance par le lanceur).
    m.kill(game);
    assert_eq!(tick(&mut m, &mut w, &mut store), Event::Cooling { remaining: 2 });
    assert_eq!(tick(&mut m, &mut w, &mut store), Event::Cooling { remaining: 1 });
    assert!(matches!(tick(&mut m, &mut w, &mut store), Event::Released { .. }));
    assert!(!w.engaged());
    assert_eq!(m.get(late).unwrap().priority, Priority::Normal);
    assert_eq!(by_name(&m, "winword.exe").priority, Priority::Normal);
    assert_eq!(m.power_plan, BALANCED_PLAN);
}

#[test]
fn game_relaunched_during_cooldown_keeps_the_same_session() {
    let cfg = Config::builtin();
    let profile = cfg.profile("gaming").unwrap();
    let mut m = office_then_game();
    let game = by_name(&m, "eldenring.exe").info.id;
    let mut store = MemStore::default();
    let mut w = Watcher::default();
    let snap = m.snapshot().unwrap();
    w.tick(&mut m, &mut store, &cfg, "gaming", profile, &snap);
    m.kill(game);
    let snap = m.snapshot().unwrap();
    assert!(matches!(
        w.tick(&mut m, &mut store, &cfg, "gaming", profile, &snap),
        Event::Cooling { .. }
    ));
    m.spawn("eldenring.exe", Some(GAME), 4 * GIB);
    let snap = m.snapshot().unwrap();
    let ev = w.tick(&mut m, &mut store, &cfg, "gaming", profile, &snap);
    assert!(!matches!(ev, Event::Engaged { .. } | Event::Released { .. }), "{ev:?}");
    assert!(w.engaged());
}

#[test]
fn cyber_profile_never_engages() {
    let cfg = Config::builtin();
    let mut m = office_then_game();
    let mut store = MemStore::default();
    let mut w = Watcher::default();
    let snap = m.snapshot().unwrap();
    assert_eq!(
        w.tick(&mut m, &mut store, &cfg, "cyber", cfg.profile("cyber").unwrap(), &snap),
        Event::Idle
    );
    assert!(m.log.is_empty(), "aucune action : {:?}", m.log);
}

#[test]
fn switching_to_cyber_mid_game_restores_immediately() {
    let cfg = Config::builtin();
    let mut m = office_then_game();
    let mut store = MemStore::default();
    let mut w = Watcher::default();
    let snap = m.snapshot().unwrap();
    w.tick(
        &mut m,
        &mut store,
        &cfg,
        "gaming",
        cfg.profile("gaming").unwrap(),
        &snap,
    );
    let snap = m.snapshot().unwrap();
    let ev = w.tick(&mut m, &mut store, &cfg, "cyber", cfg.profile("cyber").unwrap(), &snap);
    assert!(matches!(ev, Event::Released { .. }), "{ev:?}");
    assert_eq!(by_name(&m, "winword.exe").priority, Priority::Normal);
}

#[test]
fn manual_clean_frees_ram_and_puts_memory_priorities_back() {
    let cfg = Config::builtin();
    let mut m = office_then_game();
    let before = m.mem();
    let report = clean(&mut m, &cfg, false).unwrap();
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert!(m.mem().free >= before.free + 5 * GIB);
    assert_eq!(m.standby[5], 4 * GIB, "clean normal : cache du jeu gardé");
    for p in &m.procs {
        assert_eq!(
            p.mem_priority,
            MemPriority::Normal,
            "{} : priorité mémoire remise",
            p.info.name
        );
        assert_eq!(
            p.priority,
            Priority::Normal,
            "{} : clean ne touche pas au CPU",
            p.info.name
        );
    }

    let mut m = office_then_game();
    clean(&mut m, &cfg, true).unwrap();
    assert_eq!(m.standby.iter().sum::<u64>(), 0, "clean --deep : tout le cache purgé");
}

#[test]
fn a_process_already_lower_is_never_raised() {
    let cfg = Config::builtin();
    let mut m = office_then_game();
    let word = m.procs.iter_mut().find(|p| p.info.name == "winword.exe").unwrap();
    word.priority = Priority::Idle;
    word.mem_priority = MemPriority::VeryLow;
    let mut store = MemStore::default();
    let mut session = None;
    let snap = m.snapshot().unwrap();
    engage(
        &mut m,
        &mut store,
        &cfg,
        "gaming",
        cfg.profile("gaming").unwrap(),
        &mut session,
        &snap,
    );
    let word = by_name(&m, "winword.exe");
    assert_eq!(
        (word.priority, word.mem_priority),
        (Priority::Idle, MemPriority::VeryLow)
    );
    let journal = session.unwrap().journal;
    restore(&mut m, &mut store, &journal);
    let word = by_name(&m, "winword.exe");
    assert_eq!(
        (word.priority, word.mem_priority),
        (Priority::Idle, MemPriority::VeryLow),
        "toujours intact après restauration"
    );
}
