//! prism.exe — Mode Jeu, RAM, profils et outils cyber pour Windows.

mod demo;
mod render;
mod sys;

use std::process::{Command, ExitCode};
#[cfg(windows)]
use std::time::Duration;

#[cfg(windows)]
use prism_core::classify::{classify_all, games_running, Class};
use prism_core::config::{Config, DEFAULT_TOML};
#[cfg(windows)]
use prism_core::model::human_bytes;
use prism_core::tools::{install_commands, resolve_pack};

const HELP: &str = "\
Prism OS — couche gaming et cybersécurité pour Windows

Utilisation : prism <commande>

  status                  profil, mémoire, jeux détectés, conflits anti-cheat
  profile [nom]           affiche ou change le profil (gaming, balanced, cyber)
  watch [--quiet]         Mode Quotidien permanent + Mode Jeu automatique ; Ctrl-C restaure tout
  top                     qui consomme le processeur et la RAM en ce moment
  ram                     état détaillé de la mémoire
  ram clean [--deep]      libère la RAM des programmes en arrière-plan
  tools [pack]            packs d'outils cyber et leurs commandes d'installation
  tools install <pack>    installe un pack (winget, Kali sous WSL)
  demarrage               applis lancées au démarrage, avec conseils
  demarrage recommande    désactive les applis conseillées (réversible)
  demarrage off|on <nom>  désactive / réactive une entrée ; demarrage restore annule
  allege                  catalogue d'allègement (services, stratégies) et état
  allege apply [niveaux]  applique : sur (défaut), avance, jeu (admin)
  allege restore          remet toutes les valeurs d'origine (admin)
  config init|check|path  copie modifiable de la configuration
  autostart on|off        lance le Mode Jeu à l'ouverture de session (admin)
  demo                    partie simulée de bout en bout (tout système)
  version
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    match run(&words) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("prism : {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[&str]) -> Result<(), String> {
    let cfg = sys::load_config()?;
    match args {
        [] | ["help" | "--help" | "-h"] => {
            print!("{HELP}");
            Ok(())
        }
        ["version" | "--version"] => {
            println!("prism {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        ["demo"] => demo::run(&cfg),
        ["profile"] => {
            let active = sys::active_profile(&cfg);
            for (name, p) in &cfg.profiles {
                let mark = if *name == active { "●" } else { " " };
                println!("{mark} {name:<9} {} — {}", p.label, p.description);
            }
            Ok(())
        }
        ["profile", name] => {
            let p = cfg.profile(name)?;
            sys::save_state(&sys::State {
                profile: Some(name.to_string()),
            })?;
            println!("Profil actif : {} — {}", p.label, p.description);
            if !p.tool_packs.is_empty() {
                println!(
                    "Packs d'outils proposés : {} (voir `prism tools`)",
                    p.tool_packs.join(", ")
                );
            }
            Ok(())
        }
        ["tools"] => {
            for (name, pack) in &cfg.packs {
                let tools: Vec<String> = resolve_pack(&cfg, name)?.into_iter().map(|t| t.name).collect();
                println!("{name:<8} {:<22} {}", pack.label, tools.join(", "));
            }
            println!("\nDétail : prism tools <pack> · Installation : prism tools install <pack>");
            Ok(())
        }
        ["tools", "install", pack] => tools_install(&cfg, pack),
        ["allege"] => allege_list(),
        ["tools", pack] => {
            for t in resolve_pack(&cfg, pack)? {
                println!("{}", t.name);
                if let Some(reason) = &t.reason {
                    println!("  ⚠ {reason}");
                }
                for cmd in install_commands(&t) {
                    println!("  $ {}", cmd.join(" "));
                }
            }
            Ok(())
        }
        ["config", "path"] => {
            println!("{}", sys::config_path().display());
            Ok(())
        }
        ["config", "check"] => {
            sys::load_config()?;
            println!("Configuration valide ({}).", sys::config_path().display());
            Ok(())
        }
        ["config", "init"] => {
            let path = sys::config_path();
            if path.exists() {
                return Err(format!(
                    "{} existe déjà ; supprimez-le pour repartir du défaut",
                    path.display()
                ));
            }
            std::fs::create_dir_all(sys::data_dir()).map_err(|e| e.to_string())?;
            std::fs::write(&path, DEFAULT_TOML).map_err(|e| e.to_string())?;
            println!("Configuration écrite dans {}", path.display());
            Ok(())
        }
        _ => platform_command(&cfg, args),
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
fn allege_tiers(words: &[&str]) -> Result<Vec<prism_core::allege::Tier>, String> {
    use prism_core::allege::Tier;
    if words.is_empty() {
        return Ok(vec![Tier::Sur]);
    }
    words
        .iter()
        .map(|w| Tier::parse(w).ok_or_else(|| format!("niveau inconnu « {w} » (sur, avance, jeu)")))
        .collect()
}

/// Liste le catalogue ; sous Windows, avec l'état réel de chaque entrée.
fn allege_list() -> Result<(), String> {
    use prism_core::allege::{Catalog, SystemConfig};
    let c = Catalog::builtin();
    #[cfg(windows)]
    let mut sys: Option<Box<dyn SystemConfig>> = Some(Box::new(prism_win::WindowsSystemConfig));
    #[cfg(not(windows))]
    let mut sys: Option<Box<dyn SystemConfig>> = None;
    println!("Services");
    for s in &c.services {
        let state = match sys.as_mut().map(|x| x.service_start(&s.name)) {
            Some(Ok(Some(st))) => format!("{st:?}"),
            Some(Ok(None)) => "absent".into(),
            Some(Err(e)) => e,
            None => "-".into(),
        };
        println!(
            "  [{:<9}] {:<20} {:<12} -> {:<9} {}",
            s.tier.label(),
            s.name,
            state,
            format!("{:?}", s.start),
            s.label
        );
    }
    println!("Registre (stratégies et réglages)");
    for p in &c.policies {
        let state = match sys.as_mut().map(|x| x.policy(&p.full_key(), &p.value)) {
            Some(Ok(Some(d))) => d.to_string(),
            Some(Ok(None)) => "absente".into(),
            Some(Err(e)) => e,
            None => "-".into(),
        };
        println!(
            "  [{:<9}] {:<30} {:<8} -> {:<3} {}",
            p.tier.label(),
            p.value,
            state,
            p.data,
            p.label
        );
    }
    let protected: usize = c.protected.iter().map(|p| p.services.len()).sum();
    println!("\n{protected} services protégés (anti-cheats, mises à jour, sécurité) ne sont jamais touchés.");
    println!("Détail et raisons : config/allegement.toml");
    Ok(())
}

fn tools_install(cfg: &Config, pack: &str) -> Result<(), String> {
    for t in resolve_pack(cfg, pack)? {
        println!("== {}", t.name);
        if let Some(reason) = &t.reason {
            println!("   ⚠ {reason}");
        }
        for cmd in install_commands(&t) {
            println!("   $ {}", cmd.join(" "));
            let status = Command::new(&cmd[0])
                .args(&cmd[1..])
                .status()
                .map_err(|e| format!("{} introuvable : {e}", cmd[0]))?;
            if !status.success() {
                return Err(format!("{} : échec ({status}), installation arrêtée", t.name));
            }
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn platform_command(_cfg: &Config, args: &[&str]) -> Result<(), String> {
    match args.first() {
        Some(&("status" | "watch" | "ram" | "autostart" | "allege" | "top" | "demarrage")) => {
            Err("cette commande agit sur Windows ; ici, essayez `prism demo`".into())
        }
        _ => Err(format!("commande inconnue : {}\n\n{HELP}", args.join(" "))),
    }
}

#[cfg(windows)]
fn platform_command(cfg: &Config, args: &[&str]) -> Result<(), String> {
    use prism_core::platform::Platform;
    use prism_win::WindowsPlatform;

    match args {
        ["status"] => {
            let mut w = WindowsPlatform::new();
            let snap = w.snapshot()?;
            let profile = sys::active_profile(cfg);
            println!(
                "Profil : {} · {}",
                cfg.profile(&profile)?.label,
                render::memory(&snap.mem)
            );
            match prism_core::cores::split(&snap.cpus) {
                Some(s) => println!("Cœurs : {}", s.describe()),
                None => println!(
                    "Cœurs : {} cœurs logiques homogènes (pas de répartition)",
                    snap.cpus.len()
                ),
            }
            if !w.can_purge {
                println!("(sans droits administrateur : purge du cache indisponible, RAM libre approximative)");
            }
            let games = games_running(&snap, cfg);
            println!(
                "Jeux : {}",
                if games.is_empty() {
                    "aucun".to_string()
                } else {
                    games.join(", ")
                }
            );
            let classes = classify_all(&snap, cfg);
            let mut background: Vec<_> = classes.iter().filter(|(_, c)| *c == Class::Background).collect();
            let count = |k: &Class| classes.iter().filter(|(_, c)| c == k).count();
            println!(
                "Processus de la session : {} arrière-plan · {} compagnons · {} protégés",
                background.len(),
                count(&Class::Companion),
                count(&Class::Protected)
            );
            background.sort_by_key(|(p, _)| std::cmp::Reverse(p.working_set));
            for (p, _) in background.iter().take(5) {
                println!("  {:<32} {}", p.name, human_bytes(p.working_set));
            }
            render::conflicts(&prism_core::tools::conflicts(&snap, cfg));
            Ok(())
        }
        ["top"] => {
            let mut w = WindowsPlatform::new();
            let a = w.snapshot()?;
            let t0 = std::time::Instant::now();
            std::thread::sleep(Duration::from_millis(1500));
            let b = w.snapshot()?;
            let elapsed = t0.elapsed().as_secs_f64() * 1e7; // en unités de 100 ns
            let ncpu = w.cpus.len().max(1) as f64;
            let classes: std::collections::HashMap<_, _> =
                classify_all(&b, cfg).into_iter().map(|(p, c)| (p.id, c)).collect();
            let mut rows: Vec<(f64, &prism_core::model::ProcInfo)> = b
                .procs
                .iter()
                .filter_map(|p| {
                    let before = a.procs.iter().find(|q| q.id == p.id)?;
                    let d = p.cpu_time.saturating_sub(before.cpu_time) as f64;
                    Some((d / elapsed / ncpu * 100.0, p))
                })
                .collect();
            rows.sort_by(|x, y| y.0.total_cmp(&x.0));
            println!("{:<34} {:>6} {:>9}  classe", "processus", "CPU %", "RAM");
            for (cpu, p) in rows.iter().take(15) {
                let class = classes.get(&p.id).map(|c| c.label()).unwrap_or("-");
                println!("{:<34} {:>6.1} {:>9}  {class}", p.name, cpu, human_bytes(p.working_set));
            }
            Ok(())
        }
        ["ram"] => {
            let mut w = WindowsPlatform::new();
            println!("{}", render::memory(&w.snapshot()?.mem));
            Ok(())
        }
        ["ram", "clean"] | ["ram", "clean", "--deep"] => {
            let mut w = WindowsPlatform::new();
            let report = prism_core::engine::clean(&mut w, cfg, args.len() == 3)?;
            render::report(&report, "");
            if let Some(after) = report.mem_after {
                println!("{}", render::memory(&after));
            }
            Ok(())
        }
        ["watch"] => watch(cfg, false),
        ["watch", "--quiet"] => watch(cfg, true),
        ["allege", "apply", tiers @ ..] => {
            use prism_core::allege::{apply, plan, AllegeJournal, Catalog};
            let c = Catalog::builtin();
            let changes = plan(&c, &allege_tiers(tiers)?);
            let path = sys::data_dir().join("allegement.json");
            let mut journal = AllegeJournal::load(&path)?;
            let mut sysconf = prism_win::WindowsSystemConfig;
            let r = apply(&mut sysconf, &c, &changes, &mut journal, &mut |j| j.save(&path));
            for d in &r.done {
                println!("  ✓ {d}");
            }
            println!(
                "{} appliqué(s), {} déjà fait(s) ou absent(s), {} échec(s)",
                r.done.len(),
                r.unchanged.len(),
                r.failed.len()
            );
            for f in &r.failed {
                println!("  ÉCHEC : {f}");
            }
            println!("Annuler : prism allege restore");
            if r.failed.is_empty() {
                Ok(())
            } else {
                Err("certains changements ont échoué".into())
            }
        }
        ["demarrage"] => {
            use prism_core::demarrage::{is_enabled, Catalog, StartupConfig};
            let c = Catalog::builtin();
            let mut entries = prism_win::WindowsStartup.entries()?;
            entries.sort_by_key(|e| e.name.to_lowercase());
            for e in &entries {
                let state = if is_enabled(&e.approval) {
                    "activé   "
                } else {
                    "désactivé"
                };
                let (advice, why) = match (c.protection(e), c.advice(e)) {
                    (Some(reason), _) => ("protégé".to_string(), reason.to_string()),
                    (None, Some(r)) => (r.advice.label().to_string(), r.why.clone()),
                    (None, None) => ("inconnu".to_string(), String::new()),
                };
                println!(
                    "{state}  {:<34} [{:<12}] {:<15} {why}",
                    e.name,
                    advice,
                    e.source.label()
                );
            }
            println!(
                "\n{} entrée(s). « prism demarrage recommande » désactive celles marquées « à désactiver ».",
                entries.len()
            );
            Ok(())
        }
        ["demarrage", "recommande"] => {
            use prism_core::demarrage::{apply_recommended, Catalog, StartupJournal};
            let path = sys::data_dir().join("demarrage.json");
            let mut journal = StartupJournal::load(&path)?;
            let r = apply_recommended(&mut prism_win::WindowsStartup, &Catalog::builtin(), &mut journal)?;
            journal.save(&path)?;
            print_startup_report(&r);
            Ok(())
        }
        ["demarrage", verb @ ("off" | "on"), name] => {
            use prism_core::demarrage::{disable, enable, Catalog, StartupConfig, StartupJournal, StartupReport};
            let path = sys::data_dir().join("demarrage.json");
            let mut journal = StartupJournal::load(&path)?;
            let c = Catalog::builtin();
            let mut w = prism_win::WindowsStartup;
            let matching: Vec<_> = w
                .entries()?
                .into_iter()
                .filter(|e| e.name.eq_ignore_ascii_case(name))
                .collect();
            if matching.is_empty() {
                return Err(format!("aucune entrée « {name} » (voir prism demarrage)"));
            }
            let mut r = StartupReport::default();
            for e in &matching {
                if *verb == "off" {
                    disable(&mut w, &c, e, &mut journal, &mut r);
                } else {
                    enable(&mut w, e, &mut journal, &mut r);
                }
            }
            journal.save(&path)?;
            print_startup_report(&r);
            Ok(())
        }
        ["demarrage", "restore"] => {
            use prism_core::demarrage::{restore, StartupJournal};
            let path = sys::data_dir().join("demarrage.json");
            let mut journal = StartupJournal::load(&path)?;
            let r = restore(&mut prism_win::WindowsStartup, &mut journal);
            journal.save(&path)?;
            print_startup_report(&r);
            Ok(())
        }
        ["allege", "restore"] => {
            use prism_core::allege::{restore, AllegeJournal};
            let path = sys::data_dir().join("allegement.json");
            let mut journal = AllegeJournal::load(&path)?;
            if journal.originals.is_empty() {
                println!("Rien à restaurer.");
                return Ok(());
            }
            let r = restore(&mut prism_win::WindowsSystemConfig, &mut journal);
            journal.save(&path)?;
            println!(
                "{} valeur(s) d'origine remise(s), {} échec(s)",
                r.done.len(),
                r.failed.len()
            );
            for f in &r.failed {
                println!("  ÉCHEC : {f}");
            }
            if r.failed.is_empty() {
                Ok(())
            } else {
                Err("restauration incomplète, relancez en administrateur".into())
            }
        }
        ["autostart", "on"] => {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let task = format!("\"{}\" watch --quiet", exe.display());
            schtasks(&[
                "/Create", "/TN", "Prism OS", "/TR", &task, "/SC", "ONLOGON", "/RL", "HIGHEST", "/F",
            ])
        }
        ["autostart", "off"] => schtasks(&["/Delete", "/TN", "Prism OS", "/F"]),
        _ => Err(format!("commande inconnue : {}\n\n{HELP}", args.join(" "))),
    }
}

#[cfg(windows)]
fn print_startup_report(r: &prism_core::demarrage::StartupReport) {
    for d in &r.done {
        println!("  ✓ {d}");
    }
    for u in &r.unchanged {
        println!("  = {u}");
    }
    for f in &r.failed {
        println!("  ÉCHEC : {f}");
    }
    println!(
        "{} changement(s), {} inchangé(s), {} échec(s) · annuler : prism demarrage restore",
        r.done.len(),
        r.unchanged.len(),
        r.failed.len()
    );
}

#[cfg(windows)]
fn schtasks(args: &[&str]) -> Result<(), String> {
    let status = Command::new("schtasks.exe")
        .args(args)
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("schtasks a échoué (lancez la console en administrateur)".into())
    }
}

#[cfg(windows)]
fn watch(cfg: &Config, quiet: bool) -> Result<(), String> {
    use prism_core::daily::Daily;
    use prism_core::engine::recover;
    use prism_core::journal::FileStore;
    use prism_core::platform::Platform;
    use prism_core::watch::{Event, Watcher};
    use prism_win::WindowsPlatform;

    if quiet {
        sys::detach_console();
    }
    sys::install_stop_handler();
    let out = sys::Out::new(quiet);
    let mut w = WindowsPlatform::new();
    let mut store = FileStore {
        path: sys::journal_path(),
    };

    let mut daily_store = FileStore {
        path: sys::data_dir().join("quotidien.json"),
    };
    for s in [&mut store, &mut daily_store] {
        if let Some(report) = recover(&mut w, s)? {
            out.line(&format!(
                "Reprise après arrêt brutal : {} réglage(s) restauré(s), {} ignoré(s)",
                report.done.len(),
                report.skipped.len()
            ));
        }
    }
    let mut watcher = Watcher::default();
    let mut daily = Daily::default();
    out.line(&format!(
        "Prism en marche (profil {}) : Mode Quotidien permanent, Mode Jeu automatique.",
        sys::active_profile(cfg)
    ));
    if let Some(split) = prism_core::cores::split(&w.cpus) {
        out.line(&format!("Cœurs : {}", split.describe()));
    }

    while !sys::stop_requested() {
        // Le profil peut changer pendant la surveillance (`prism profile cyber`).
        let profile_name = sys::active_profile(cfg);
        let profile = cfg.profile(&profile_name)?.clone();
        match w.snapshot() {
            Ok(snap) => {
                let event = watcher.tick(&mut w, &mut store, cfg, &profile_name, &profile, &snap);
                let r = daily.tick(
                    &mut w,
                    &mut daily_store,
                    cfg,
                    &profile,
                    &snap,
                    cfg.poll_seconds,
                    watcher.engaged(),
                );
                if !r.done.is_empty() || !r.failed.is_empty() {
                    out.line(&format!("Quotidien ({} appli(s) allégée(s)) :", daily.eased()));
                    for d in &r.done {
                        out.line(&format!("  {d}"));
                    }
                    for f in &r.failed {
                        out.line(&format!("  ÉCHEC : {f}"));
                    }
                }
                match event {
                    Event::Engaged {
                        games,
                        report,
                        conflicts,
                    } => {
                        out.line(&format!("Mode Jeu : {}", games.join(", ")));
                        log_report(&out, &report);
                        for c in conflicts {
                            out.line(&format!("⚠ {} ouvert ({}) : {}", c.tool, c.process, c.reason));
                        }
                    }
                    Event::Updated { report } => log_report(&out, &report),
                    Event::Released { report } => {
                        out.line("Fin du Mode Jeu, réglages restaurés.");
                        log_report(&out, &report);
                    }
                    Event::Idle | Event::Cooling { .. } => {}
                }
            }
            Err(e) => out.line(&format!("relevé impossible : {e}")),
        }
        sys::sleep_interruptible(Duration::from_secs(cfg.poll_seconds));
    }
    if let Event::Released { report } = watcher.release(&mut w, &mut store) {
        out.line("Arrêt : réglages du Mode Jeu restaurés.");
        log_report(&out, &report);
    }
    let r = daily.release(&mut w, &mut daily_store);
    if !r.done.is_empty() {
        out.line(&format!(
            "Arrêt : {} réglage(s) du Mode Quotidien rendus.",
            r.done.len()
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn log_report(out: &sys::Out, r: &prism_core::engine::Report) {
    out.line(&format!(
        "  {} fait · {} ignoré · {} échec",
        r.done.len(),
        r.skipped.len(),
        r.failed.len()
    ));
    for f in &r.failed {
        out.line(&format!("  ÉCHEC : {f}"));
    }
}
