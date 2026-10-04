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
  jeux                    jeux installés (Steam, Epic, GOG, Battle.net)
  jeux lancer <nom>       lance un jeu par son magasin
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
  vie-privee              tableau de bord : protections en place, télémétrie qui parle en ce moment
  vie-privee apply [niveau]  applique : recommande (défaut) ou strict (admin)
  vie-privee restore      remet tout comme avant (admin)
  apparence               animations, effets, thème (réglages officiels de Windows)
  apparence <préréglage>  performance | fluide ; apparence set <id> <option> ; restore
  config init|check|path  copie modifiable de la configuration
  autostart on|off        lance le Mode Jeu à l'ouverture de session (admin)
  bar on|off              lance / arrête la Prism Bar ; bar autostart on|off
  fx demo | fx stats      démonstration mesurée des effets ; mesures des dernières animations
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
/// Tableau de bord vie privée, par catégorie.
#[cfg_attr(not(windows), allow(dead_code))]
fn print_privacy(rows: &[prism_core::privacy::Row]) {
    use prism_core::privacy::{score, Category, Kind, State};
    for cat in Category::ALL {
        let list: Vec<_> = rows.iter().filter(|r| r.category == cat).collect();
        if list.is_empty() {
            continue;
        }
        println!("{}", cat.label());
        for r in list {
            let mark = match &r.state {
                State::On => "✓",
                State::Off => "·",
                State::Absent => "–",
                State::Unknown(_) => "?",
            };
            let level = match (r.kind, r.level) {
                (Kind::Check, _) => format!("via {}", r.source),
                (_, Some(l)) => l.label().to_string(),
                _ => String::new(),
            };
            let kind = if r.kind == Kind::Firewall { " [pare-feu]" } else { "" };
            println!("  {mark} {}{kind}  ({level})", r.label);
            if let State::Unknown(e) = &r.state {
                println!("      {e}");
            }
        }
    }
    let (on, total) = score(rows);
    println!("\n{on} protection(s) en place sur {total}  (✓ en place · non appliquée – composant absent)");
}

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
    println!("Tâches planifiées");
    for t in &c.tasks {
        let state = match sys.as_mut().map(|x| x.task_enabled(&t.path)) {
            Some(Ok(Some(true))) => "active".to_string(),
            Some(Ok(Some(false))) => "désactivée".to_string(),
            Some(Ok(None)) => "absente".to_string(),
            Some(Err(e)) => e,
            None => "-".into(),
        };
        println!("  [{:<9}] {:<11} {}", t.tier.label(), state, t.label);
    }
    let protected: usize = c.protected.iter().map(|p| p.services.len()).sum();
    println!("\n{protected} services protégés (anti-cheats, mises à jour, sécurité) ne sont jamais touchés.");
    println!("Détail et raisons : config/allegement.toml");
    Ok(())
}

#[cfg_attr(not(windows), allow(dead_code))]
/// Minuscules sans accents : une option se trouve même si la console a abîmé
/// les accents (« Instantanee », « instantanée » et « Instantanée » se valent).
fn fold(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            other => other,
        })
        .filter(|c| c.is_ascii_alphanumeric() || *c == ' ' || *c == '(' || *c == ')')
        .collect()
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
        Some(
            &("status" | "watch" | "ram" | "autostart" | "allege" | "top" | "demarrage" | "jeux" | "apparence" | "bar"
            | "vie-privee"),
        ) => Err("cette commande agit sur Windows ; ici, essayez `prism demo`".into()),
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
            if snap.user_session == prism_core::classify::SERVICES_SESSION {
                println!(
                    "⚠ Prism tourne dans la session des services (SSH ou service) : aucun processus ne sera allégé. Lancez-le depuis votre session (prism autostart on)."
                );
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
        ["jeux"] => {
            let games = prism_win::installed_games();
            for g in &games {
                let how = match &g.launch {
                    prism_core::library::Launch::None => " (détecté, lancement par son magasin)",
                    _ => "",
                };
                println!("{:<11} {:<40} {}{how}", g.store.label(), g.name, g.install_dir);
            }
            println!("\n{} jeu(x). Lancer : prism jeux lancer <nom>", games.len());
            Ok(())
        }
        ["jeux", "lancer", query @ ..] => {
            use prism_core::library::{find, Launch};
            let games = prism_win::installed_games();
            let q = query.join(" ");
            let g = find(&games, &q).ok_or_else(|| format!("aucun jeu « {q} » (voir prism jeux)"))?;
            let status = match &g.launch {
                Launch::Uri(u) => Command::new("explorer.exe").arg(u).status(),
                Launch::Exe(e) => Command::new(e).status(),
                Launch::None => return Err(format!("{} : lancez-le depuis {}", g.name, g.store.label())),
            };
            status.map_err(|e| e.to_string())?;
            println!("Lancement de {} ({})", g.name, g.store.label());
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
            use prism_core::demarrage::{Catalog, StartupConfig};
            let c = Catalog::builtin();
            let mut entries = prism_win::WindowsStartup.entries()?;
            entries.sort_by_key(|e| e.name.to_lowercase());
            for e in &entries {
                let state = if e.enabled() { "activé   " } else { "désactivé" };
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
        ["apparence"] => {
            use prism_core::apparence::{current, Catalog};
            let c = Catalog::builtin();
            let cur = current(&mut prism_win::WindowsAppearance, &c);
            for g in c.groups() {
                println!("{g}");
                for k in c.knobs.iter().filter(|k| k.group == g) {
                    let active = match cur.get(&k.id) {
                        Some(Ok(Some(v))) => k
                            .options
                            .iter()
                            .find(|o| &o.value == v)
                            .map(|o| o.label.clone())
                            .unwrap_or_else(|| format!("{v:?}")),
                        Some(Ok(None)) => "non défini".into(),
                        Some(Err(e)) => e.clone(),
                        None => "-".into(),
                    };
                    let opts: Vec<&str> = k.options.iter().map(|o| o.label.as_str()).collect();
                    println!("  {:<16} {:<44} {:<18} [{}]", k.id, k.label, active, opts.join(" | "));
                }
            }
            let presets: Vec<&str> = c.presets.iter().map(|p| p.id.as_str()).collect();
            println!(
                "\nPréréglages : {} · prism apparence <préréglage> · set <id> <option> · restore",
                presets.join(", ")
            );
            Ok(())
        }
        ["apparence", "restore"] => {
            apparence_run(|sys, j| prism_core::apparence::restore(sys, &prism_core::apparence::Catalog::builtin(), j))
        }
        ["apparence", "set", id, option @ ..] => {
            use prism_core::apparence::Catalog;
            let c = Catalog::builtin();
            let k = c.knob(id).ok_or_else(|| format!("réglage inconnu « {id} »"))?.clone();
            let wanted = fold(&option.join(" "));
            let o = k
                .options
                .iter()
                .enumerate()
                .find(|(i, o)| fold(&o.label) == wanted || (i + 1).to_string() == wanted)
                .map(|(_, o)| o)
                .ok_or_else(|| format!("option inconnue « {wanted} » pour {id}"))?
                .value
                .clone();
            apparence_run(move |sys, j| {
                prism_core::apparence::apply(
                    sys,
                    &c,
                    &std::collections::BTreeMap::from([(k.id.clone(), o.clone())]),
                    j,
                )
            })
        }
        ["apparence", preset] => {
            let c = prism_core::apparence::Catalog::builtin();
            let p = c
                .preset(preset)
                .ok_or_else(|| format!("préréglage inconnu « {preset} »"))?
                .clone();
            apparence_run(move |sys, j| prism_core::apparence::apply(sys, &c, &p.values, j))
        }
        ["vie-privee"] => {
            use prism_core::privacy::{status, Catalog};
            let c = Catalog::builtin();
            let rows = status(&mut prism_win::WindowsPrivacy::new(), &c);
            print_privacy(&rows);
            let conns = prism_win::telemetry_connections(&c);
            if conns.is_empty() {
                println!("\nTélémétrie en ce moment : aucune connexion ouverte par les composants surveillés.");
            } else {
                println!("\nTélémétrie en ce moment : {} connexion(s)", conns.len());
                for k in &conns {
                    println!("  {:<28} {:<22} {}", k.component, k.remote, k.state);
                }
            }
            Ok(())
        }
        ["vie-privee", "apply", rest @ ..] => {
            use prism_core::privacy::{apply, journal_path, plan, Catalog, Journal, Level};
            let level = match rest {
                [] => Level::Recommande,
                [l] => Level::parse(l).ok_or_else(|| format!("niveau inconnu « {l} » (recommande, strict)"))?,
                _ => return Err("un seul niveau : recommande ou strict".into()),
            };
            let c = Catalog::builtin();
            let path = journal_path();
            let mut journal = Journal::load(&path)?;
            let r = apply(
                &mut prism_win::WindowsPrivacy::new(),
                &plan(&c, level),
                &mut journal,
                &mut |j| j.save(&path),
            );
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
            if c.registry.iter().any(|x| x.reboot && x.level <= level) {
                println!("Certains réglages s'appliquent à la prochaine ouverture de session.");
            }
            println!("Annuler : prism vie-privee restore");
            if r.failed.is_empty() {
                Ok(())
            } else {
                Err("certains changements ont échoué (droits administrateur ?)".into())
            }
        }
        ["vie-privee", "restore"] => {
            use prism_core::privacy::{journal_path, restore, Journal};
            let path = journal_path();
            let mut journal = Journal::load(&path)?;
            if journal.originals.is_empty() {
                println!("Rien à restaurer.");
                return Ok(());
            }
            let r = restore(&mut prism_win::WindowsPrivacy::new(), &mut journal);
            journal.save(&path)?;
            println!("{} élément(s) remis, {} échec(s)", r.done.len(), r.failed.len());
            for f in &r.failed {
                println!("  ÉCHEC : {f}");
            }
            if r.failed.is_empty() {
                Ok(())
            } else {
                Err("restauration incomplète, relancez en administrateur".into())
            }
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
        ["fx", "demo"] => {
            if !prism_win::bar_app::fx_demo() {
                return Err("la Prism Bar doit tourner (prism bar on)".into());
            }
            println!("Démonstration lancée sur la fenêtre au premier plan ; mesures dans quelques secondes…");
            std::thread::sleep(Duration::from_secs(6));
            print!("{}", prism_core::fx::summarize(&prism_core::fx::load_stats()));
            Ok(())
        }
        ["fx", "stats"] => {
            let stats = prism_core::fx::load_stats();
            if stats.is_empty() {
                println!("Aucune mesure : activez les effets (prism-ui, Apparence) ou lancez prism fx demo.");
            } else {
                print!("{}", prism_core::fx::summarize(&stats));
            }
            Ok(())
        }
        ["bar", "on"] => {
            let exe = std::env::current_exe()
                .map_err(|e| e.to_string())?
                .with_file_name("prism-bar.exe");
            Command::new(&exe)
                .spawn()
                .map_err(|e| format!("{} : {e}", exe.display()))?;
            println!("Prism Bar lancée (réglages : prism-ui, page Apparence)");
            Ok(())
        }
        ["bar", "off"] => {
            if prism_win::bar_app::stop() {
                println!("Prism Bar arrêtée, barre des tâches Windows remise");
            } else {
                println!("Prism Bar ne tournait pas");
            }
            Ok(())
        }
        ["bar", "autostart", "on"] => {
            let exe = std::env::current_exe()
                .map_err(|e| e.to_string())?
                .with_file_name("prism-bar.exe");
            let task = format!("\"{}\"", exe.display());
            schtasks(&["/Create", "/TN", "Prism Bar", "/TR", &task, "/SC", "ONLOGON", "/F"])
        }
        ["bar", "autostart", "off"] => schtasks(&["/Delete", "/TN", "Prism Bar", "/F"]),
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
fn apparence_run(
    f: impl FnOnce(
        &mut prism_win::WindowsAppearance,
        &mut prism_core::apparence::AppearanceJournal,
    ) -> prism_core::apparence::AppearanceReport,
) -> Result<(), String> {
    let path = sys::data_dir().join("apparence.json");
    let mut j = prism_core::apparence::AppearanceJournal::load(&path)?;
    let r = f(&mut prism_win::WindowsAppearance, &mut j);
    j.save(&path)?;
    for d in &r.done {
        println!("  ✓ {d}");
    }
    for f in &r.failed {
        println!("  ÉCHEC : {f}");
    }
    println!(
        "{} changé(s), {} inchangé(s), {} échec(s) · annuler : prism apparence restore",
        r.done.len(),
        r.unchanged.len(),
        r.failed.len()
    );
    if r.failed.is_empty() {
        Ok(())
    } else {
        Err("certains réglages ont échoué".into())
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
    // Les dossiers des jeux installés deviennent des racines de jeu : un jeu installé
    // hors des dossiers habituels (Battle.net, GOG, disque secondaire) est reconnu.
    let mut cfg = cfg.clone();
    let library = prism_win::installed_games();
    cfg.lists.game_roots.extend(prism_core::library::game_roots(&library));
    cfg.lists.game_roots.sort();
    cfg.lists.game_roots.dedup();
    let cfg = &cfg;
    out.line(&format!(
        "Bibliothèque : {} jeu(x) installé(s) détecté(s).",
        library.len()
    ));
    let mut watcher = Watcher::default();
    let mut daily = Daily::default();
    let mut etat = prism_core::etat::Etat {
        cores: prism_core::cores::split(&w.cpus).map(|s| s.describe()),
        ..Default::default()
    };
    let mut written = prism_core::etat::Etat::default();
    let mut ticks: u64 = 0;
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
                let mut lines: Vec<String> = r.done.iter().map(|d| format!("Quotidien : {d}")).collect();
                match &event {
                    Event::Engaged { games, .. } => lines.push(format!("Mode Jeu : {}", games.join(", "))),
                    Event::Released { .. } => lines.push("Fin du Mode Jeu, réglages restaurés".into()),
                    _ => {}
                }
                for l in lines {
                    etat.push_recent(format!("{} {l}", &sys::now_utc()[11..19]));
                }
                etat.profile = profile_name.clone();
                etat.game = watcher.games.clone();
                etat.eased = daily.eased_names(&snap);
                etat.mem = snap.mem;
                ticks += 1;
                if !etat.same_content(&written) || ticks % 5 == 0 {
                    etat.updated_unix = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    if etat.save().is_ok() {
                        written = etat.clone();
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

#[cfg(test)]
mod cli_tests {
    #[test]
    fn options_match_without_accents() {
        assert_eq!(super::fold("Instantanée"), super::fold("instantanee"));
        assert_eq!(super::fold("Vif (100 ms)"), "vif (100 ms)");
    }
}
