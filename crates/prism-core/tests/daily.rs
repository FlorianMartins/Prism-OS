//! Mode Quotidien et gestion avancée des processus, sur la plateforme simulée.

use prism_core::config::Config;
use prism_core::daily::Daily;
use prism_core::engine::{engage, recover, restore};
use prism_core::journal::{JournalStore, MemStore};
use prism_core::mock::MockPlatform;
use prism_core::model::{CpuInfo, EcoState, MemPriority, Priority, ProcId};
use prism_core::platform::Platform;
use prism_core::watch::{Event, Watcher};

const GIB: u64 = 1024 * 1024 * 1024;
const POLL: u64 = 2;
const GAME: &str = r"C:\XboxGames\Forza Horizon 5\Content\ForzaHorizon5.exe";

struct Bench {
    m: MockPlatform,
    d: Daily,
    store: MemStore,
    cfg: Config,
}

impl Bench {
    fn new() -> Bench {
        Bench {
            m: MockPlatform::new(16 * GIB),
            d: Daily::default(),
            store: MemStore::default(),
            cfg: Config::builtin(),
        }
    }

    /// Fait passer `secs` secondes de surveillance (passes de 2 s).
    fn run(&mut self, secs: u64, game_running: bool) -> prism_core::engine::Report {
        let profile = self.cfg.profile("gaming").unwrap().clone();
        let mut all = prism_core::engine::Report::default();
        for _ in 0..secs / POLL {
            let snap = self.m.snapshot().unwrap();
            let r = self.d.tick(
                &mut self.m,
                &mut self.store,
                &self.cfg,
                &profile,
                &snap,
                POLL,
                game_running,
            );
            all.merge(r);
        }
        all
    }

    fn proc(&self, id: ProcId) -> &prism_core::mock::MockProc {
        self.m.get(id).unwrap()
    }
}

#[test]
fn an_idle_app_is_eased_after_5_minutes_then_its_ram_is_returned_after_30() {
    let mut b = Bench::new();
    let word = b.m.spawn("WINWORD.EXE", None, 2 * GIB);
    b.run(4 * 60, false);
    assert_eq!(b.proc(word).eco, EcoState::SystemManaged, "4 min : pas encore");
    b.run(2 * 60, false);
    let w = b.proc(word);
    assert_eq!(
        (w.eco, w.mem_priority),
        (EcoState::On, MemPriority::Low),
        "6 min : allégé"
    );
    assert_eq!(w.info.working_set, 2 * GIB, "pas encore rogné");
    assert_eq!(
        w.priority,
        Priority::Normal,
        "le Quotidien ne touche pas à la priorité CPU"
    );
    b.run(25 * 60, false);
    assert_eq!(b.proc(word).info.working_set, 0, "31 min : RAM rendue");
    b.run(2 * POLL, false);
    assert_eq!(
        b.proc(word).eco,
        EcoState::On,
        "le rognage lui-même ne doit pas faire croire à une reprise d'activité"
    );
}

#[test]
fn a_busy_app_or_the_foreground_app_is_never_eased() {
    let mut b = Bench::new();
    let render = b.m.spawn("blender.exe", None, GIB);
    let browser = b.m.spawn("firefox.exe", None, GIB);
    b.m.foreground = Some(browser.pid);
    for _ in 0..(10 * 60 / POLL) {
        b.m.work(render, 500); // 25 % d'un cœur
        b.run(POLL, false);
    }
    assert_eq!(b.proc(render).eco, EcoState::SystemManaged);
    assert_eq!(b.proc(browser).eco, EcoState::SystemManaged);
}

#[test]
fn coming_back_to_an_app_gives_it_back_immediately() {
    let mut b = Bench::new();
    let word = b.m.spawn("winword.exe", None, GIB);
    b.run(6 * 60, false);
    assert_eq!(b.proc(word).eco, EcoState::On);
    assert!(!b.d.journal.entries.is_empty());

    b.m.foreground = Some(word.pid);
    let r = b.run(POLL, false);
    let w = b.proc(word);
    assert_eq!((w.eco, w.mem_priority), (EcoState::SystemManaged, MemPriority::Normal));
    assert!(r.done.iter().any(|d| d.starts_with("rendu")));
    assert!(b.d.journal.entries.is_empty());
    assert_eq!(
        b.store.load().unwrap(),
        None,
        "journal effacé quand plus rien n'est allégé"
    );

    // L'utilisateur repart : le compteur repart de zéro.
    b.m.foreground = None;
    b.run(4 * 60, false);
    assert_eq!(b.proc(word).eco, EcoState::SystemManaged);
    b.run(2 * 60, false);
    assert_eq!(b.proc(word).eco, EcoState::On);
}

#[test]
fn an_app_that_starts_working_again_is_given_back() {
    let mut b = Bench::new();
    let sync = b.m.spawn("onedrive.exe", None, GIB / 4);
    b.run(6 * 60, false);
    assert_eq!(b.proc(sync).eco, EcoState::On);
    b.m.work(sync, 2_000);
    b.run(POLL, false);
    assert_eq!(b.proc(sync).eco, EcoState::SystemManaged);
}

#[test]
fn protected_companion_and_games_are_never_eased() {
    let mut b = Bench::new();
    let ids = [
        b.m.spawn("discord.exe", None, GIB),
        b.m.spawn("vgc.exe", None, GIB / 10),
        b.m.spawn("explorer.exe", None, GIB / 10),
        b.m.spawn("forzahorizon5.exe", Some(GAME), 4 * GIB),
    ];
    b.run(60 * 60, false);
    for id in ids {
        let p = b.proc(id);
        assert_eq!(
            (p.eco, p.mem_priority, p.info.working_set > 0),
            (EcoState::SystemManaged, MemPriority::Normal, true),
            "{}",
            p.info.name
        );
    }
}

#[test]
fn a_closed_app_is_forgotten_without_errors() {
    let mut b = Bench::new();
    let word = b.m.spawn("winword.exe", None, GIB);
    b.run(6 * 60, false);
    b.m.kill(word);
    let r = b.run(POLL, false);
    assert!(r.failed.is_empty() && r.skipped.is_empty(), "{r:?}");
    assert!(b.d.journal.entries.is_empty());
}

#[test]
fn hybrid_cpu_sends_idle_apps_to_e_cores() {
    let mut b = Bench::new();
    b.m.cpus = (0..16)
        .map(|i| CpuInfo {
            id: i,
            efficiency_class: u8::from(i < 8),
            llc_bytes: 30 << 20,
        })
        .collect();
    let word = b.m.spawn("winword.exe", None, GIB);
    b.run(6 * 60, false);
    assert_eq!(
        b.proc(word).cpu_sets,
        (8..16).collect::<Vec<u32>>(),
        "cœurs économes = classe la plus basse"
    );
    b.m.foreground = Some(word.pid);
    b.run(POLL, false);
    assert!(b.proc(word).cpu_sets.is_empty(), "tous les cœurs rendus");
}

#[test]
fn during_a_game_no_new_app_is_eased_but_foreground_is_still_given_back() {
    let mut b = Bench::new();
    let early = b.m.spawn("winword.exe", None, GIB);
    b.run(6 * 60, false);
    let late = b.m.spawn("excel.exe", None, GIB);
    b.run(10 * 60, true);
    assert_eq!(b.proc(late).eco, EcoState::SystemManaged, "le Mode Jeu s'en charge");
    b.m.work(early, 5_000);
    b.run(POLL, true);
    assert_eq!(
        b.proc(early).eco,
        EcoState::On,
        "en jeu, une appli qui travaille seule reste allégée"
    );
    b.m.foreground = Some(early.pid);
    b.run(POLL, true);
    assert_eq!(
        b.proc(early).eco,
        EcoState::SystemManaged,
        "ramenée au premier plan : rendue"
    );
}

#[test]
fn turning_daily_off_gives_everything_back() {
    let mut b = Bench::new();
    let word = b.m.spawn("winword.exe", None, GIB);
    b.run(6 * 60, false);
    let mut off = b.cfg.profile("gaming").unwrap().clone();
    off.daily = false;
    let snap = b.m.snapshot().unwrap();
    b.d.tick(&mut b.m, &mut b.store, &b.cfg, &off, &snap, POLL, false);
    assert_eq!(b.proc(word).eco, EcoState::SystemManaged);
    assert!(b.d.journal.entries.is_empty());
}

#[test]
fn a_crash_while_apps_are_eased_is_recovered() {
    let mut b = Bench::new();
    let word = b.m.spawn("winword.exe", None, GIB);
    b.run(6 * 60, false);
    // Prism meurt : seul le journal quotidien reste.
    let report = recover(&mut b.m, &mut b.store).unwrap().expect("journal quotidien");
    assert!(report.failed.is_empty());
    let w = b.proc(word);
    assert_eq!((w.eco, w.mem_priority), (EcoState::SystemManaged, MemPriority::Normal));
}

#[test]
fn ram_watch_purges_when_memory_gets_tight() {
    let mut b = Bench::new();
    b.m.total = 8 * GIB;
    b.m.spawn("winword.exe", None, 3 * GIB);
    b.m.spawn("game-launcher-helper.exe", None, 4 * GIB);
    let free_before = b.m.mem().free;
    let r = b.run(31 * 60, false);
    assert!(r.done.iter().any(|d| d.contains("surveillance RAM")), "{:?}", r.done);
    assert!(b.m.mem().free >= free_before + 6 * GIB);
}

// --- Mode Jeu : cœurs, services, gel ---------------------------------------

fn game_bench() -> (MockPlatform, Config) {
    let mut m = MockPlatform::new(32 * GIB);
    m.cpus = (0..32)
        .map(|i| CpuInfo {
            id: i,
            efficiency_class: 0,
            llc_bytes: if i < 16 { 96 << 20 } else { 32 << 20 },
        })
        .collect();
    m.services.insert("wsearch".into(), true);
    m.spawn("winword.exe", None, GIB);
    m.spawn("onedrive.exe", None, GIB / 4);
    m.spawn("forzahorizon5.exe", Some(GAME), 8 * GIB);
    let mut cfg = Config::builtin();
    cfg.lists.suspend_in_game = vec!["onedrive.exe".into()];
    (m, cfg)
}

#[test]
fn game_mode_uses_the_non_vcache_ccd_pauses_indexing_and_freezes_listed_apps_then_restores() {
    let (mut m, cfg) = game_bench();
    let profile = cfg.profile("gaming").unwrap();
    let mut store = MemStore::default();
    let mut session = None;
    let snap = m.snapshot().unwrap();
    let r = engage(&mut m, &mut store, &cfg, "gaming", profile, &mut session, &snap);
    assert!(r.failed.is_empty(), "{:?}", r.failed);

    let by = |m: &MockPlatform, n: &str| m.procs.iter().find(|p| p.info.name == n).unwrap().clone();
    assert_eq!(
        by(&m, "winword.exe").cpu_sets,
        (16..32).collect::<Vec<u32>>(),
        "puce sans cache 3D"
    );
    assert!(
        by(&m, "forzahorizon5.exe").cpu_sets.is_empty(),
        "le jeu n'est jamais touché"
    );
    assert!(by(&m, "onedrive.exe").suspended);
    assert!(
        !by(&m, "winword.exe").suspended,
        "seules les applis désignées sont gelées"
    );
    assert!(!m.services["wsearch"], "indexation en pause");

    let journal = session.unwrap().journal;
    let r = restore(&mut m, &mut store, &journal);
    assert!(r.failed.is_empty());
    assert!(by(&m, "winword.exe").cpu_sets.is_empty());
    assert!(!by(&m, "onedrive.exe").suspended);
    assert!(m.services["wsearch"], "indexation relancée");
}

#[test]
fn a_service_already_stopped_is_not_restarted_after_the_game() {
    let (mut m, cfg) = game_bench();
    m.services.insert("wsearch".into(), false);
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
    restore(&mut m, &mut store, &session.unwrap().journal);
    assert!(!m.services["wsearch"], "on ne relance que ce qu'on a arrêté");
}

#[test]
fn cpu_sets_chosen_by_the_user_are_respected() {
    let (mut m, cfg) = game_bench();
    m.procs
        .iter_mut()
        .find(|p| p.info.name == "winword.exe")
        .unwrap()
        .cpu_sets = vec![3];
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
    assert_eq!(
        m.procs.iter().find(|p| p.info.name == "winword.exe").unwrap().cpu_sets,
        vec![3]
    );
}

#[test]
fn a_protected_service_cannot_be_paused_by_config() {
    let bad =
        prism_core::config::DEFAULT_TOML.replace("pause_services = [\"WSearch\"]", "pause_services = [\"Winmgmt\"]");
    let err = Config::parse(&bad).unwrap_err();
    assert!(err.contains("Winmgmt") && err.contains("protégé"), "{err}");
}

#[test]
fn game_mode_and_daily_together_leave_the_machine_exactly_as_it_was() {
    let (mut m, cfg) = game_bench();
    let original: Vec<_> = m
        .procs
        .iter()
        .map(|p| (p.info.id, p.priority, p.eco, p.mem_priority, p.cpu_sets.clone()))
        .collect();
    let profile = cfg.profile("gaming").unwrap().clone();
    let (mut daily, mut dstore) = (Daily::default(), MemStore::default());
    let (mut watcher, mut gstore) = (Watcher::default(), MemStore::default());
    let game = m
        .procs
        .iter()
        .find(|p| p.info.name == "forzahorizon5.exe")
        .unwrap()
        .info
        .id;
    m.kill(game); // d'abord une heure de bureautique, sans jeu
    for _ in 0..(40 * 60 / POLL) {
        let snap = m.snapshot().unwrap();
        watcher.tick(&mut m, &mut gstore, &cfg, "gaming", &profile, &snap);
        daily.tick(&mut m, &mut dstore, &cfg, &profile, &snap, POLL, watcher.engaged());
    }
    m.spawn("forzahorizon5.exe", Some(GAME), 8 * GIB);
    for _ in 0..30 {
        let snap = m.snapshot().unwrap();
        watcher.tick(&mut m, &mut gstore, &cfg, "gaming", &profile, &snap);
        daily.tick(&mut m, &mut dstore, &cfg, &profile, &snap, POLL, watcher.engaged());
    }
    assert!(watcher.engaged());
    assert!(matches!(watcher.release(&mut m, &mut gstore), Event::Released { .. }));
    daily.release(&mut m, &mut dstore);
    let now: Vec<_> = m
        .procs
        .iter()
        .filter(|p| p.info.name != "forzahorizon5.exe")
        .map(|p| (p.info.id, p.priority, p.eco, p.mem_priority, p.cpu_sets.clone()))
        .collect();
    let before: Vec<_> = original.into_iter().filter(|o| o.0 != game).collect();
    assert_eq!(now, before);
    assert!(m.procs.iter().all(|p| !p.suspended));
    assert!(m.services["wsearch"]);
}
