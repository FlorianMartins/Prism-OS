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
  allege apply [niveaux]  applique : sur (défaut), avance, jeu, extreme (admin)
  jeu-noyau [on|off]      plan automatique pour les jeux à anti-cheat noyau
  webview [on|off]        WebView des applis sans fenêtre : liste, ou fermeture auto
  tools uninstall <x>     désinstalle un outil ou un pack (Kali : efface la distribution)
  rapport                 où part la mémoire (fichier texte à envoyer, sans données personnelles)
  allege restore          remet toutes les valeurs d'origine (admin)
  vie-privee              tableau de bord : protections en place, télémétrie qui parle en ce moment
  vie-privee apply [niveau]  applique : recommande (défaut) ou strict (admin)
  vie-privee restore      remet tout comme avant (admin)
  apparence               animations, effets, thème (réglages officiels de Windows)
  apparence <préréglage>  performance | fluide ; apparence set <id> <option> ; restore
  config init|check|path  copie modifiable de la configuration
  config export <fichier> toute la configuration (barre, thème, apparence, niveaux) dans un fichier
  config import <fichier> la réapplique, sur ce PC ou un autre (admin pour les niveaux)
  autostart on|off        lance le Mode Jeu à l'ouverture de session (admin)
  desinstaller            remet TOUT comme avant Prism (fait aussi par la désinstallation, admin)
  maj                     y a-t-il une version plus récente ? (GitHub)
  maj installer           la télécharge, vérifie son empreinte SHA-256 et l'installe (admin)
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
        ["tools", "uninstall", what] => tools_uninstall(&cfg, what),
        ["allege"] => allege_list(),
        ["jeu-noyau", rest @ ..] => jeu_noyau(rest),
        ["webview", "on" | "off"] => {
            let dir = prism_core::paths::user_dir();
            let mut r = prism_core::webview::Reglages::charger(&dir);
            r.actif = args[1] == "on";
            r.enregistrer(&dir)?;
            println!(
                "Fermeture des WebView en arrière-plan : {}",
                if r.actif { "active" } else { "désactivée" }
            );
            Ok(())
        }
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

/// `prism jeu-noyau [on|off]` : réglages du plan automatique (fichier de l'utilisateur,
/// lu par le moteur à chaque partie protégée).
fn jeu_noyau(args: &[&str]) -> Result<(), String> {
    use prism_core::noyau::{Reglages, ANTICHEATS};
    let dir = prism_core::paths::user_dir();
    let mut r = Reglages::charger(&dir);
    match args {
        [] => {}
        ["on"] => r.actif = true,
        ["off"] => r.actif = false,
        _ => return Err("usage : prism jeu-noyau [on|off]".into()),
    }
    if !args.is_empty() {
        r.enregistrer(&dir)?;
    }
    let oui = |b: bool| if b { "oui" } else { "non" };
    println!(
        "Plan « jeu noyau » : {}",
        if r.actif { "ACTIF (automatique)" } else { "désactivé" }
    );
    println!(
        "  remettre les services du niveau Extrême     {}",
        oui(r.services_extreme)
    );
    println!(
        "  fermer les outils qui gênent les anti-cheats {}",
        oui(r.fermer_outils_genants)
    );
    println!(
        "  fermer tous les outils (VM comprises)       {}",
        oui(r.fermer_tous_les_outils)
    );
    println!(
        "  arrêter services et pilotes des outils      {}",
        oui(r.arreter_services_outils)
    );
    println!("  éteindre WSL                                {}", oui(r.eteindre_wsl));
    if !r.exclus.is_empty() {
        println!("  outils jamais touchés : {}", r.exclus.join(", "));
    }
    let mut noms: Vec<&str> = ANTICHEATS.iter().map(|(_, l)| *l).collect();
    noms.dedup();
    println!("Anti-cheats reconnus : {}", noms.join(", "));
    println!(
        "Réglages détaillés : appli Prism, page Allègement ({}).",
        dir.join(Reglages::FICHIER).display()
    );
    Ok(())
}

#[cfg_attr(not(windows), allow(dead_code))]
fn allege_tiers(words: &[&str]) -> Result<Vec<prism_core::allege::Tier>, String> {
    use prism_core::allege::Tier;
    if words.is_empty() {
        return Ok(vec![Tier::Sur]);
    }
    words
        .iter()
        .map(|w| Tier::parse(w).ok_or_else(|| format!("niveau inconnu « {w} » (sur, avance, jeu, extreme)")))
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
    println!("Applis préinstallées (retirées pour l'utilisateur, réinstallables depuis le Store)");
    for a in &c.apps {
        let state = match sys.as_mut().map(|x| x.app_installed(&a.package)) {
            Some(Ok(true)) => "installée".to_string(),
            Some(Ok(false)) => "absente".to_string(),
            Some(Err(e)) => e,
            None => "-".into(),
        };
        println!("  [{:<9}] {:<11} {}", a.tier.label(), state, a.label);
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

/// `prism tools uninstall <outil|pack>` : mêmes commandes que l'appli (silencieuses).
fn tools_uninstall(cfg: &Config, what: &str) -> Result<(), String> {
    use prism_core::tools::{uninstall_commands, uninstall_erases_data};
    let tools: Vec<_> = match cfg.packs.get(what) {
        Some(p) => p.tools.iter().filter_map(|id| cfg.tool(id).cloned()).collect(),
        None => vec![cfg
            .tool(what)
            .cloned()
            .ok_or_else(|| format!("outil ou pack inconnu « {what} »"))?],
    };
    for t in &tools {
        if uninstall_erases_data(t) {
            println!("⚠ {} : la distribution et tous ses fichiers vont être effacés.", t.name);
        }
        for cmd in uninstall_commands(t) {
            println!("== {} : {}", t.name, cmd.join(" "));
            let status = Command::new(&cmd[0])
                .args(&cmd[1..])
                .status()
                .map_err(|e| format!("{} introuvable : {e}", cmd[0]))?;
            if !status.success() {
                println!("   échec ({status})");
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
            | "vie-privee" | "config" | "maj" | "desinstaller" | "webview" | "rapport"),
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
        ["rapport"] => {
            let (path, text) = rapport()?;
            println!("{text}");
            println!("Rapport enregistré : {}", path.display());
            println!("Envoyez ce fichier tel quel : il ne contient aucune donnée personnelle.");
            Ok(())
        }
        ["webview"] => {
            use prism_core::platform::Platform;
            let snap = prism_win::WindowsPlatform::new().snapshot()?;
            let r = prism_core::webview::Reglages::charger(&prism_core::paths::user_dir());
            let gs = prism_core::webview::groupes(&snap);
            if gs.is_empty() {
                println!("Aucune WebView ouverte dans votre session.");
            }
            for g in &gs {
                println!(
                    "{:<28} {:>5} Mo  {}",
                    g.owner_name,
                    g.bytes >> 20,
                    if g.windowed {
                        "fenêtre ouverte"
                    } else {
                        "sans fenêtre"
                    }
                );
            }
            let total: u64 = gs.iter().map(|g| g.bytes).sum();
            println!("\nTotal : {} Mo.", total >> 20);
            println!(
                "Fermeture automatique : {} (après {} min sans fenêtre ; prism webview on|off).",
                if r.actif { "active" } else { "désactivée" },
                r.minutes
            );
            Ok(())
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
        ["config", "export", file] => {
            use prism_core::{allege, apparence, backup, privacy};
            let config_toml = std::fs::read_to_string(sys::config_path()).ok();
            let ac = apparence::Catalog::builtin();
            let appearance = apparence::current(&mut prism_win::WindowsAppearance, &ac)
                .into_iter()
                .filter_map(|(id, v)| v.ok().flatten().map(|v| (id, v)))
                .collect();
            let rows = privacy::status(&mut prism_win::WindowsPrivacy::new(), &privacy::Catalog::builtin());
            let b = backup::Backup {
                format: backup::FORMAT,
                prism_version: env!("CARGO_PKG_VERSION").into(),
                bar: prism_core::bar::BarConfig::load(),
                config_toml,
                appearance,
                allege: backup::allege_applied(&mut prism_win::WindowsSystemConfig, &allege::Catalog::builtin()),
                privacy: backup::privacy_applied(&rows),
            };
            std::fs::write(file, backup::to_json(&b)?).map_err(|e| format!("{file} : {e}"))?;
            println!("Configuration exportée dans {file}");
            println!("  barre : bord {:?}, thème {}", b.bar.edge, b.bar.theme.label());
            println!(
                "  configuration de Prism : {}",
                if b.config_toml.is_some() {
                    "personnalisée"
                } else {
                    "par défaut"
                }
            );
            println!("  apparence : {} réglage(s)", b.appearance.len());
            let tiers: Vec<&str> = b.allege.iter().map(|t| t.label()).collect();
            println!(
                "  allègement appliqué : {}",
                if tiers.is_empty() {
                    "aucun".into()
                } else {
                    tiers.join(", ")
                }
            );
            println!(
                "  vie privée : {}",
                b.privacy.map(|l| l.label()).unwrap_or("aucun niveau complet")
            );
            Ok(())
        }
        ["config", "import", file] => {
            use prism_core::{allege, apparence, backup, privacy};
            let bytes = std::fs::read(file).map_err(|e| format!("{file} : {e}"))?;
            let (b, warnings) = backup::parse(&bytes)?;
            for w in &warnings {
                println!("  ⚠ {w}");
            }
            let mut failed = 0usize;
            b.bar.save()?;
            println!("✓ barre et thème ({})", b.bar.theme.label());
            if let Some(t) = &b.config_toml {
                let path = sys::config_path();
                if path.exists() {
                    std::fs::copy(&path, path.with_extension("toml.bak")).map_err(|e| e.to_string())?;
                }
                std::fs::create_dir_all(sys::data_dir()).map_err(|e| e.to_string())?;
                std::fs::write(&path, t).map_err(|e| e.to_string())?;
                println!("✓ configuration de Prism (l'ancienne est gardée en config.toml.bak)");
            }
            if !b.appearance.is_empty() {
                let ac = apparence::Catalog::builtin();
                let path = prism_core::paths::user_dir().join("apparence.json");
                let mut j = apparence::AppearanceJournal::load(&path)?;
                let r = apparence::apply(&mut prism_win::WindowsAppearance, &ac, &b.appearance, &mut j);
                j.save(&path)?;
                failed += r.failed.len();
                println!(
                    "✓ apparence : {} changé(s), {} déjà en place, {} échec(s)",
                    r.done.len(),
                    r.unchanged.len(),
                    r.failed.len()
                );
            }
            if !b.allege.is_empty() {
                let c = allege::Catalog::builtin();
                let path = sys::data_dir().join("allegement.json");
                let mut j = allege::AllegeJournal::load(&path)?;
                let r = allege::apply(
                    &mut prism_win::WindowsSystemConfig,
                    &c,
                    &allege::plan(&c, &b.allege),
                    &mut j,
                    &mut |j| j.save(&path),
                );
                failed += r.failed.len();
                println!(
                    "✓ allègement : {} appliqué(s), {} déjà fait(s), {} échec(s)",
                    r.done.len(),
                    r.unchanged.len(),
                    r.failed.len()
                );
            }
            if let Some(level) = b.privacy {
                let c = privacy::Catalog::builtin();
                let path = privacy::journal_path();
                let mut j = privacy::Journal::load(&path)?;
                let r = privacy::apply(
                    &mut prism_win::WindowsPrivacy::new(),
                    &privacy::plan(&c, level),
                    &mut j,
                    &mut |j| j.save(&path),
                );
                failed += r.failed.len();
                println!(
                    "✓ vie privée « {} » : {} appliqué(s), {} déjà fait(s), {} échec(s)",
                    level.label(),
                    r.done.len(),
                    r.unchanged.len(),
                    r.failed.len()
                );
            }
            println!("Tout reste réversible : apparence, allege et vie-privee restore.");
            if failed == 0 {
                Ok(())
            } else {
                Err(format!("{failed} changement(s) refusé(s) (droits administrateur ?)"))
            }
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
            detached(&mut Command::new(&exe))
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
        ["apres-installation", rest @ ..] => {
            // Lancé par l'installateur MSI (compte SYSTEM) : `prism` dans tout terminal.
            let dir = install_dir()?;
            match prism_win::install::path_add(&dir)? {
                true => println!("{dir} ajouté au PATH du système"),
                false => println!("{dir} déjà dans le PATH"),
            }
            // Tâche « Prism (admin) » de l'utilisateur, à la demande, avec les droits
            // administrateur : l'appli s'y relance seule, sans fenêtre de confirmation à
            // chaque ouverture (sans droits, Prism ne pouvait presque rien faire — retour
            // d'un vrai utilisateur). `--depuis-tache` empêche toute boucle de relance.
            let quiet =
                rest.get(1).and_then(|l| l.parse::<u32>().ok()).map_or(true, |l| l < 5) && rest.get(2) != Some(&"1");
            if let Some(user) = rest.first().filter(|u| !u.is_empty()) {
                let ui = format!("\"{dir}\\prism-ui.exe\" --depuis-tache");
                let created = schtasks(&[
                    "/Create",
                    "/TN",
                    ADMIN_TASK,
                    "/TR",
                    &ui,
                    "/SC",
                    "ONCE",
                    "/ST",
                    "00:00",
                    "/SD",
                    "01/01/2020",
                    "/RU",
                    user,
                    "/IT",
                    "/RL",
                    "HIGHEST",
                    "/F",
                ]);
                // Installation sans assistant : l'appli s'ouvre (avec l'assistant ou
                // Prism-Setup.exe, c'est leur case « Lancer Prism » qui s'en charge).
                if created.is_ok() && quiet {
                    let _ = schtasks(&["/Run", "/TN", ADMIN_TASK]);
                }
            }
            Ok(())
        }
        ["desinstaller", rest @ ..] => uninstall(rest),
        ["perf"] => {
            // Coût d'un relevé du moteur (diagnostic, non documenté dans l'aide).
            use prism_core::platform::Platform;
            let mut w = prism_win::WindowsPlatform::new();
            let first = std::time::Instant::now();
            let n = w.snapshot()?.procs.len();
            let cold = first.elapsed();
            let t0 = std::time::Instant::now();
            for _ in 0..50 {
                w.snapshot()?;
            }
            let warm = t0.elapsed() / 50;
            println!(
                "relevé de {n} processus : premier {:.2} ms, suivants {:.2} ms en moyenne",
                cold.as_secs_f64() * 1e3,
                warm.as_secs_f64() * 1e3
            );
            Ok(())
        }
        ["maj"] => match check_update()? {
            None => {
                println!("Prism {} est à jour.", env!("CARGO_PKG_VERSION"));
                Ok(())
            }
            Some(u) => {
                println!(
                    "Nouvelle version : {} (installée : {})\n{}\nInstaller : prism maj installer",
                    u.version,
                    env!("CARGO_PKG_VERSION"),
                    u.page
                );
                Ok(())
            }
        },
        ["maj", "installer"] => update_install(),
        ["maj", "--appliquer", msi, bar, engine] => update_apply(msi, *bar == "1", *engine == "1"),
        _ => Err(format!("commande inconnue : {}\n\n{HELP}", args.join(" "))),
    }
}

/// Processus lancé sans hériter de nos entrées/sorties.
#[cfg(windows)]
fn detached(cmd: &mut Command) -> &mut Command {
    use std::process::Stdio;
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
}

/// Téléchargement par `curl.exe` (fourni avec Windows 10 et 11) : pas de pile HTTPS
/// embarquée dans Prism.
#[cfg(windows)]
fn download(url: &str, to: Option<&std::path::Path>) -> Result<Vec<u8>, String> {
    let mut cmd = Command::new("curl.exe");
    cmd.args(["-fsSL", "--max-time", "300", "-A", "prism-os"]);
    if let Some(p) = to {
        cmd.arg("-o").arg(p);
    }
    let out = cmd.arg(url).output().map_err(|e| format!("curl.exe : {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "téléchargement impossible ({url}) : {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(out.stdout)
}

#[cfg(windows)]
fn check_update() -> Result<Option<prism_core::update::Update>, String> {
    let json = download(&prism_core::update::latest_url(), None)?;
    prism_core::update::from_release(&json, env!("CARGO_PKG_VERSION"))
}

/// Télécharge et vérifie la nouvelle version, puis passe la main à une copie de Prism
/// hors du dossier d'installation (un programme en cours d'exécution ne peut pas être
/// remplacé).
#[cfg(windows)]
fn update_install() -> Result<(), String> {
    use prism_core::update::{expected_sha256, hex, sha256};
    let Some(u) = check_update()? else {
        println!("Prism {} est à jour.", env!("CARGO_PKG_VERSION"));
        return Ok(());
    };
    let dir = std::env::temp_dir().join("prism-maj");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let msi = dir.join(&u.msi_name);
    println!("Téléchargement de Prism {}…", u.version);
    let sums = String::from_utf8_lossy(&download(&u.sums_url, None)?).into_owned();
    download(&u.msi_url, Some(&msi))?;
    let expected = expected_sha256(&sums, &u.msi_name).ok_or("empreinte de l'installateur absente de SHA256SUMS")?;
    let got = sha256(&std::fs::read(&msi).map_err(|e| e.to_string())?);
    if got != expected {
        let _ = std::fs::remove_file(&msi);
        return Err(format!(
            "empreinte différente : attendu {}, reçu {} — fichier supprimé, rien n'est installé",
            hex(&expected),
            hex(&got)
        ));
    }
    println!("✓ empreinte SHA-256 vérifiée ({})", hex(&got));
    let helper = dir.join("prism-maj.exe");
    std::fs::copy(std::env::current_exe().map_err(|e| e.to_string())?, &helper).map_err(|e| e.to_string())?;
    let bar = prism_win::bar_app::running();
    let engine = prism_win::install::other_instances("prism.exe") > 0;
    detached(Command::new(&helper).args(["maj", "--appliquer", &msi.display().to_string()]))
        .arg(if bar { "1" } else { "0" })
        .arg(if engine { "1" } else { "0" })
        .spawn()
        .map_err(|e| e.to_string())?;
    println!("Installation de la mise à jour lancée ; la barre et le moteur redémarrent ensuite.");
    Ok(())
}

/// Lancé depuis la copie temporaire : arrête la barre et le moteur (journaux appliqués),
/// installe le MSI en mise à jour (les réglages et journaux sont gardés), puis relance
/// ce qui tournait.
#[cfg(windows)]
fn update_apply(msi: &str, bar: bool, engine: bool) -> Result<(), String> {
    std::thread::sleep(std::time::Duration::from_millis(800));
    if prism_win::bar_app::stop() {
        for _ in 0..20 {
            if !prism_win::bar_app::running() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
    }
    if prism_win::install::stop_other_instances("prism.exe") > 0 {
        std::thread::sleep(std::time::Duration::from_millis(500));
        use prism_core::journal::FileStore;
        let mut w = prism_win::WindowsPlatform::new();
        for path in [sys::journal_path(), sys::data_dir().join("quotidien.json")] {
            let _ = prism_core::engine::recover(&mut w, &mut FileStore { path });
        }
    }
    let log = std::env::temp_dir().join("prism-maj").join("msiexec.log");
    let status = Command::new("msiexec.exe")
        .args(["/i", msi, "/passive", "/norestart", "/l*v"])
        .arg(&log)
        .status()
        .map_err(|e| e.to_string())?;
    let code = status.code().unwrap_or(-1);
    let dir = std::path::PathBuf::from(std::env::var("ProgramFiles").unwrap_or_else(|_| "C:\\Program Files".into()))
        .join("Prism");
    // Relancés sans hériter des entrées/sorties de la mise à jour : sinon ils gardent
    // ouverts ses fichiers ou sa console (vu en VM : un journal resté verrouillé).
    if bar {
        let _ = detached(&mut Command::new(dir.join("prism-bar.exe"))).spawn();
    }
    if engine {
        let _ = detached(Command::new(dir.join("prism.exe")).args(["watch", "--quiet"])).spawn();
    }
    // 3010 : réussi, redémarrage conseillé.
    if code == 0 || code == 3010 {
        Ok(())
    } else {
        Err(format!("msiexec a échoué (code {code}), journal : {}", log.display()))
    }
}

/// Hors partie, intervalle du relevé complet des processus (secondes).
#[cfg(windows)]
const FULL_SCAN_SECS: u64 = 10;

/// Tâche à la demande qui ouvre l'appli avec les droits administrateur, sans invite.
#[cfg(windows)]
const ADMIN_TASK: &str = "Prism (admin)";

#[cfg(windows)]
fn install_dir() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    Ok(exe
        .parent()
        .ok_or("dossier d'installation introuvable")?
        .display()
        .to_string())
}

/// Remet tout comme avant Prism. Depuis l'installateur (compte SYSTEM, session des
/// services), la partie « utilisateur » est relancée dans la session de l'utilisateur
/// avec les droits administrateur — c'est là que vivent sa barre, ses réglages
/// d'apparence et sa branche HKCU du registre —, puis le PATH est nettoyé.
#[cfg(windows)]
fn uninstall(args: &[&str]) -> Result<(), String> {
    let done_flag = sys::data_dir().join("desinstallation-terminee");
    let service = prism_win::install::in_service_session();
    if service && args.first() != Some(&"--ici") {
        if let Some(user) = args.first().filter(|u| !u.is_empty()) {
            let _ = std::fs::remove_file(&done_flag);
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let task = format!("\"{}\" desinstaller --ici", exe.display());
            let created = schtasks(&[
                "/Create",
                "/TN",
                "Prism desinstaller",
                "/TR",
                &task,
                "/SC",
                "ONCE",
                "/ST",
                "23:59",
                "/RU",
                user,
                "/IT",
                "/RL",
                "HIGHEST",
                "/F",
            ]);
            if created.is_ok() && schtasks(&["/Run", "/TN", "Prism desinstaller"]).is_ok() {
                // Attend la fin (deux minutes au plus) : les fichiers ne doivent pas
                // disparaître pendant qu'elle tourne.
                for _ in 0..240 {
                    if done_flag.exists() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(500));
                }
            }
            let _ = schtasks(&["/Delete", "/TN", "Prism desinstaller", "/F"]);
            let _ = std::fs::remove_file(&done_flag);
        } else {
            println!("Aucun utilisateur connecté : réglages de session non remis (relancer « prism desinstaller »).");
        }
        let _ = schtasks(&["/Delete", "/TN", "Prism OS", "/F"]);
        let _ = schtasks(&["/Delete", "/TN", "Prism Bar", "/F"]);
        let _ = schtasks(&["/Delete", "/TN", ADMIN_TASK, "/F"]);
        prism_win::install::path_remove(&install_dir()?)?;
        return Ok(());
    }
    let mut failed = 0usize;
    // 1. Le moteur : arrêté, puis ses journaux appliqués (comme à son redémarrage).
    let stopped = prism_win::install::stop_other_instances("prism.exe");
    if stopped > 0 {
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    {
        use prism_core::journal::FileStore;
        let mut w = prism_win::WindowsPlatform::new();
        for path in [sys::journal_path(), sys::data_dir().join("quotidien.json")] {
            let mut store = FileStore { path };
            if let Ok(Some(r)) = prism_core::engine::recover(&mut w, &mut store) {
                println!("✓ moteur : {} réglage(s) de processus remis", r.done.len());
            }
        }
    }
    // 2. La barre : sa fermeture rend la barre Windows, l'opacité et les fenêtres en tuiles.
    if prism_win::bar_app::stop() {
        for _ in 0..20 {
            if !prism_win::bar_app::running() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        println!("✓ Prism Bar arrêtée, barre des tâches Windows remise");
    }
    // 3. Chaque module, par son journal.
    {
        use prism_core::privacy;
        let path = privacy::journal_path();
        let mut j = privacy::Journal::load(&path)?;
        let r = privacy::restore(&mut prism_win::WindowsPrivacy::new(), &mut j);
        j.save(&path)?;
        failed += r.failed.len();
        println!("✓ vie privée : {} remis, {} échec(s)", r.done.len(), r.failed.len());
    }
    if let Some(e) = prism_core::noyau::Etat::charger(&sys::data_dir()) {
        noyau_sortir(&e);
    }
    {
        use prism_core::allege::{restore, AllegeJournal};
        let path = sys::data_dir().join("allegement.json");
        let mut j = AllegeJournal::load(&path)?;
        let r = restore(&mut prism_win::WindowsSystemConfig, &mut j);
        j.save(&path)?;
        failed += r.failed.len();
        println!("✓ allègement : {} remis, {} échec(s)", r.done.len(), r.failed.len());
    }
    {
        let path = prism_core::paths::user_dir().join("apparence.json");
        let mut j = prism_core::apparence::AppearanceJournal::load(&path)?;
        let r = prism_core::apparence::restore(
            &mut prism_win::WindowsAppearance,
            &prism_core::apparence::Catalog::builtin(),
            &mut j,
        );
        j.save(&path)?;
        failed += r.failed.len();
        println!("✓ apparence : {} remis, {} échec(s)", r.done.len(), r.failed.len());
    }
    {
        use prism_core::demarrage::{restore, StartupJournal};
        let path = sys::data_dir().join("demarrage.json");
        let mut j = StartupJournal::load(&path)?;
        let r = restore(&mut prism_win::WindowsStartup, &mut j);
        j.save(&path)?;
        failed += r.failed.len();
        println!("✓ démarrage : {} remis, {} échec(s)", r.done.len(), r.failed.len());
    }
    let _ = schtasks(&["/Delete", "/TN", "Prism OS", "/F"]);
    let _ = schtasks(&["/Delete", "/TN", "Prism Bar", "/F"]);
    if !service {
        let _ = prism_win::install::path_remove(&install_dir()?);
    }
    let _ = std::fs::write(&done_flag, b"ok");
    if failed == 0 {
        println!("Tout est remis comme avant Prism.");
        Ok(())
    } else {
        Err(format!("{failed} élément(s) non remis (droits administrateur ?)"))
    }
}

#[cfg(windows)]
fn apparence_run(
    f: impl FnOnce(
        &mut prism_win::WindowsAppearance,
        &mut prism_core::apparence::AppearanceJournal,
    ) -> prism_core::apparence::AppearanceReport,
) -> Result<(), String> {
    let path = prism_core::paths::user_dir().join("apparence.json");
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

/// `prism rapport` : collecte, mise en forme, fichier sur le Bureau (et dans le dossier
/// de l'utilisateur). Rend le chemin et le texte.
#[cfg(windows)]
pub fn rapport() -> Result<(std::path::PathBuf, String), String> {
    use prism_core::demarrage::StartupConfig;
    use prism_core::platform::Platform;
    use prism_core::rapport::Donnees;
    use prism_win::rapport as r;
    let (processus, by_pid, compressee) = r::processus();
    let mut d = Donnees {
        date: sys::now_utc()[..16].replace('T', " ") + " UTC",
        prism: env!("CARGO_PKG_VERSION").into(),
        windows: r::windows(),
        processeur: r::processeur(),
        allume_depuis_secs: r::allume_depuis_secs(),
        memoire: r::memoire(compressee),
        hotes: r::hotes(&by_pid),
        processus,
        ..Default::default()
    };
    if let Ok(snap) = prism_win::WindowsPlatform::new().snapshot() {
        d.webviews = prism_core::webview::groupes(&snap)
            .into_iter()
            .map(|g| (g.owner_name, g.bytes, g.windowed))
            .collect();
    }
    if let Ok(entries) = prism_win::WindowsStartup.entries() {
        let mut names: Vec<String> = entries.iter().filter(|e| e.enabled()).map(|e| e.name.clone()).collect();
        names.sort_by_key(|n| n.to_lowercase());
        names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        d.demarrage = names;
    }
    let tiers = prism_core::backup::allege_applied(
        &mut prism_win::WindowsSystemConfig,
        &prism_core::allege::Catalog::builtin(),
    );
    let user = prism_core::paths::user_dir();
    let noyau = prism_core::noyau::Reglages::charger(&user);
    let wv = prism_core::webview::Reglages::charger(&user);
    let moteur = d
        .processus
        .iter()
        .any(|p| p.nom.eq_ignore_ascii_case("prism.exe") && p.session != 0);
    let prism_mem: u64 = d
        .processus
        .iter()
        .filter(|p| p.nom.to_lowercase().starts_with("prism"))
        .map(|p| p.ws)
        .sum();
    d.etat_prism = vec![
        format!(
            "moteur {}",
            if moteur {
                "en marche"
            } else {
                "ARRÊTÉ (Mode Quotidien et Mode Jeu inactifs)"
            }
        ),
        format!(
            "allègement appliqué : {}",
            if tiers.is_empty() {
                "aucun".to_string()
            } else {
                tiers.iter().map(|t| t.label()).collect::<Vec<_>>().join(", ")
            }
        ),
        format!(
            "jeux à anti-cheat noyau : {}",
            if noyau.actif { "automatique" } else { "désactivé" }
        ),
        format!(
            "WebView en arrière-plan : {}{}",
            if wv.actif {
                format!("fermées après {} min", wv.minutes)
            } else {
                "désactivé".into()
            },
            if wv.exclus.is_empty() {
                String::new()
            } else {
                format!(" (sauf {})", wv.exclus.join(", "))
            }
        ),
        format!("mémoire de Prism lui-même : {} Mo", prism_mem >> 20),
    ];
    let text = prism_core::rapport::texte(&d);
    let name = format!("Prism-rapport-{}.txt", &d.date[..10]);
    std::fs::create_dir_all(&user).map_err(|e| e.to_string())?;
    std::fs::write(user.join(&name), &text).map_err(|e| e.to_string())?;
    let desktop = std::env::var("USERPROFILE").map(|h| std::path::PathBuf::from(h).join("Desktop"));
    let path = match desktop {
        Ok(dir) if dir.is_dir() && std::fs::write(dir.join(&name), &text).is_ok() => dir.join(&name),
        _ => user.join(&name),
    };
    Ok((path, text))
}

/// Plan « jeu noyau » (prism_core::noyau) : entrée et sortie, journal d'allègement
/// relu et réécrit à chaque fois (l'appli peut l'avoir changé entre-temps).
#[cfg(windows)]
fn noyau_entrer(
    cfg: &Config,
    snap: &prism_core::model::Snapshot,
    anticheat: &str,
) -> (prism_core::noyau::Etat, Vec<String>) {
    use prism_core::allege::{AllegeJournal, Catalog};
    use prism_core::noyau;
    let path = sys::data_dir().join("allegement.json");
    let mut log = Vec::new();
    let mut journal = AllegeJournal::load(&path).unwrap_or_default();
    let reglages = noyau::Reglages::charger(&prism_core::paths::user_dir());
    let etat = noyau::entrer(
        &mut prism_win::WindowsSystemConfig,
        &mut prism_win::veille::WindowsVeille,
        &Catalog::builtin(),
        &mut journal,
        cfg,
        snap,
        &reglages,
        anticheat,
        &mut log,
    );
    if let Err(e) = journal.save(&path) {
        log.push(format!("ÉCHEC : journal d'allègement non écrit ({e})"));
    }
    if let Err(e) = etat.enregistrer(&sys::data_dir()) {
        log.push(format!("ÉCHEC : état du plan non écrit ({e})"));
    }
    (etat, log)
}

#[cfg(windows)]
fn noyau_sortir(etat: &prism_core::noyau::Etat) -> Vec<String> {
    use prism_core::allege::{AllegeJournal, Catalog};
    let path = sys::data_dir().join("allegement.json");
    let mut log = Vec::new();
    let mut journal = AllegeJournal::load(&path).unwrap_or_default();
    prism_core::noyau::sortir(
        &mut prism_win::WindowsSystemConfig,
        &mut prism_win::veille::WindowsVeille,
        &Catalog::builtin(),
        &mut journal,
        etat,
        &mut |j| j.save(&path),
        &mut log,
    );
    if let Err(e) = journal.save(&path) {
        log.push(format!("ÉCHEC : journal d'allègement non écrit ({e})"));
    }
    prism_core::noyau::Etat::effacer(&sys::data_dir());
    log
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
    // Arrêt brutal pendant une partie protégée : on rejoue la sortie.
    if let Some(e) = prism_core::noyau::Etat::charger(&sys::data_dir()) {
        out.line("Reprise : fin du plan « jeu noyau » interrompu.");
        for l in noyau_sortir(&e) {
            out.line(&format!("  {l}"));
        }
    }
    let mut noyau_etat: Option<prism_core::noyau::Etat> = None;
    let mut webviews = prism_core::webview::Reaper::default();
    let mut watcher = Watcher::default();
    let mut daily = Daily::default();
    let mut etat = prism_core::etat::Etat {
        cores: prism_core::cores::split(&w.cpus).map(|s| s.describe()),
        ..Default::default()
    };
    let mut written = prism_core::etat::Etat::default();
    let mut written_at = std::time::Instant::now();
    // Relevé complet au plus toutes les 10 s hors partie (vérification légère à chaque passe).
    let mut cadence = prism_core::cadence::Cadence::new(FULL_SCAN_SECS.max(cfg.poll_seconds));
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
        let busy = watcher.session.is_some();
        let Some(since_full) = cadence.due(w.process_ids(), busy, cfg.poll_seconds) else {
            sys::sleep_interruptible(Duration::from_secs(cfg.poll_seconds));
            continue;
        };
        match w.snapshot() {
            Ok(snap) => {
                let event = watcher.tick(&mut w, &mut store, cfg, &profile_name, &profile, &snap);
                let r = daily.tick(
                    &mut w,
                    &mut daily_store,
                    cfg,
                    &profile,
                    &snap,
                    since_full.max(cfg.poll_seconds),
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
                // WebView des applis restées sans fenêtre : fermées, sauf si l'appli la
                // recrée aussitôt (elle passe alors dans les exclusions, visibles dans l'appli).
                let wv = prism_core::webview::Reglages::charger(&prism_core::paths::user_dir());
                for d in webviews.tick(&snap, cfg, &wv, since_full.max(cfg.poll_seconds)) {
                    let l = match d {
                        prism_core::webview::Decision::Fermer(g) => {
                            let ok = g
                                .roots
                                .iter()
                                .filter(|id| prism_win::terminate_verified(id).is_ok())
                                .count();
                            format!(
                                "{}WebView de {} fermée (≈ {} Mo, sans fenêtre depuis {} min)",
                                if ok == g.roots.len() { "" } else { "ÉCHEC partiel : " },
                                g.owner_name,
                                g.bytes >> 20,
                                wv.minutes
                            )
                        }
                        prism_core::webview::Decision::Recreee { name, bytes } => {
                            let dir = prism_core::paths::user_dir();
                            let mut r = prism_core::webview::Reglages::charger(&dir);
                            if !r.exclus.contains(&name) {
                                r.exclus.push(name.clone());
                                let _ = r.enregistrer(&dir);
                            }
                            format!(
                                "{name} recrée sa WebView (≈ {} Mo) : Prism ne la fermera plus. Pour gagner cette mémoire, quittez l'appli ou retirez-la du démarrage.",
                                bytes >> 20
                            )
                        }
                    };
                    out.line(&l);
                    lines.push(l);
                }
                // Anti-cheat noyau pendant une partie : plan automatique ; fin de partie : sortie.
                match (&noyau_etat, watcher.engaged()) {
                    (None, true) => {
                        if let Some(ac) = prism_core::noyau::anticheat(&snap) {
                            let (e, log) = noyau_entrer(cfg, &snap, ac);
                            out.line(&format!("Anti-cheat noyau détecté ({ac}) :"));
                            for l in &log {
                                out.line(&format!("  {l}"));
                            }
                            lines.push(format!("{ac} : {} action(s) pour la partie", log.len()));
                            noyau_etat = Some(e);
                        }
                    }
                    (Some(e), false) => {
                        out.line("Fin de la partie protégée :");
                        for l in noyau_sortir(e) {
                            out.line(&format!("  {l}"));
                        }
                        lines.push("Fin de partie protégée : services et outils repris".into());
                        noyau_etat = None;
                    }
                    _ => {}
                }
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
                // Écrit si l'état a changé, sinon une fois par relevé complet espacé
                // (signe de vie lu par l'appli).
                if !etat.same_content(&written) || written_at.elapsed() >= Duration::from_secs(FULL_SCAN_SECS) {
                    etat.updated_unix = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    if etat.save().is_ok() {
                        written = etat.clone();
                        written_at = std::time::Instant::now();
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
    if let Some(e) = noyau_etat.take() {
        out.line("Arrêt : fin du plan « jeu noyau ».");
        for l in noyau_sortir(&e) {
            out.line(&format!("  {l}"));
        }
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
