//! Backend Windows : mêmes fonctions et mêmes journaux que `prism.exe`, donc un
//! réglage fait ici s'annule aussi par `prism … restore`, et inversement.

use std::process::Command;
use std::time::Instant;

use prism_core::allege::{self, Catalog as AllegeCatalog, SystemConfig, Tier};
use prism_core::classify::classify_all;
use prism_core::config::Config;
use prism_core::demarrage::{self, Catalog as StartupCatalog, Source, StartupConfig, StartupJournal, StartupReport};
use prism_core::etat::Etat;
use prism_core::library::{Game, Launch};
use prism_core::model::{human_bytes, Snapshot};
use prism_core::paths::{active_profile, data_dir, load_config, save_state, State};
use prism_core::platform::Platform;
use prism_win::{installed_games, WindowsPlatform, WindowsStartup, WindowsSystemConfig};

use crate::backend::{AllegeRow, Backend, Live, PackInfo, ProfileInfo, StartupRow, TopProc};

pub struct WinBackend {
    cfg: Config,
    platform: WindowsPlatform,
    previous: Option<(Snapshot, Instant)>,
    /// Mesures du processeur et de la carte graphique (comme la barre).
    metrics: prism_win::metrics::Metrics,
    /// Page Outils cyber : état partagé avec les fils de lecture et d'installation.
    tools: std::sync::Arc<std::sync::Mutex<crate::backend::ToolsView>>,
}

impl WinBackend {
    pub fn new() -> WinBackend {
        WinBackend {
            cfg: load_config().unwrap_or_else(|_| Config::builtin()),
            platform: WindowsPlatform::new(),
            previous: None,
            metrics: prism_win::metrics::Metrics::new(),
            tools: Default::default(),
        }
    }
}

/// Outils installés : un seul `winget list` (quelques secondes) et `wsl -l -q`.
fn scan_tools(cfg: &Config) -> Vec<String> {
    use prism_core::config::ToolSource;
    let winget = hidden("winget.exe")
        .args(["list", "--accept-source-agreements", "--disable-interactivity"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    // `wsl -l -q` écrit en UTF-16.
    let wsl = hidden("wsl.exe")
        .args(["-l", "-q"])
        .output()
        .map(|o| {
            let u: Vec<u16> = o
                .stdout
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            String::from_utf16_lossy(&u)
        })
        .unwrap_or_default();
    let distros = prism_core::tools::wsl_distros(&wsl);
    let ids: Vec<&str> = cfg
        .tools
        .iter()
        .filter(|t| t.source == ToolSource::Winget)
        .map(|t| t.package.as_str())
        .collect();
    let found = prism_core::tools::winget_installed(&winget, &ids);
    let kali = distros
        .iter()
        .any(|d| d.eq_ignore_ascii_case(prism_core::tools::KALI_DISTRO));
    cfg.tools
        .iter()
        .filter(|t| match t.source {
            ToolSource::Winget => found.iter().any(|f| f.eq_ignore_ascii_case(&t.package)),
            ToolSource::WslDistro => distros.iter().any(|d| d.eq_ignore_ascii_case(&t.package)),
            // Paquets dans Kali : supposés présents avec Kali (les lire demanderait de
            // démarrer la machine virtuelle).
            ToolSource::KaliApt => kali,
            ToolSource::External => false,
        })
        .map(|t| t.id.clone())
        .collect()
}

impl Default for WinBackend {
    fn default() -> Self {
        Self::new()
    }
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Commande lancée sans fenêtre de console (l'appli n'en a pas).
fn hidden(exe: &str) -> Command {
    use std::os::windows::process::CommandExt;
    let mut c = Command::new(exe);
    c.creation_flags(0x0800_0000);
    c
}

fn prism_exe() -> std::path::PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("prism.exe")))
        .unwrap_or_else(|| "prism.exe".into())
}

fn startup_summary(r: &StartupReport) -> Result<String, String> {
    if r.failed.is_empty() {
        Ok(format!("{} changement(s)", r.done.len()))
    } else {
        Err(r.failed.join(" · "))
    }
}

impl Backend for WinBackend {
    fn elevated(&self) -> bool {
        prism_win::is_elevated()
    }

    fn relaunch_elevated(&mut self) -> Result<String, String> {
        prism_win::relaunch_elevated()?;
        std::process::exit(0);
    }

    fn live(&mut self) -> Live {
        let snap = self.platform.snapshot().unwrap_or_default();
        let classes: std::collections::HashMap<_, _> = classify_all(&snap, &self.cfg)
            .into_iter()
            .map(|(p, c)| (p.id, c.label().to_string()))
            .collect();
        let ncpu = self.platform.cpus.len().max(1) as f64;
        let mut top: Vec<TopProc> = match &self.previous {
            Some((before, t0)) => {
                let elapsed = t0.elapsed().as_secs_f64().max(0.1) * 1e7;
                snap.procs
                    .iter()
                    .filter_map(|p| {
                        let b = before.procs.iter().find(|q| q.id == p.id)?;
                        let cpu = p.cpu_time.saturating_sub(b.cpu_time) as f64 / elapsed / ncpu * 100.0;
                        Some(TopProc {
                            name: p.name.clone(),
                            cpu_percent: cpu as f32,
                            ram: p.working_set,
                            class: classes.get(&p.id).cloned().unwrap_or_default(),
                        })
                    })
                    .collect()
            }
            None => Vec::new(),
        };
        top.sort_by(|a, b| b.cpu_percent.total_cmp(&a.cpu_percent).then(b.ram.cmp(&a.ram)));
        top.truncate(12);
        let etat = Etat::load();
        let watch_alive = etat.as_ref().is_some_and(|e| e.alive(now_unix(), 30));
        let cores = prism_core::cores::split(&snap.cpus)
            .map(|s| s.describe())
            .unwrap_or_else(|| format!("{} cœurs logiques homogènes", snap.cpus.len()));
        let sample = self.metrics.sample();
        let live = Live {
            profile: active_profile(&self.cfg),
            mem: snap.mem,
            top,
            cores,
            etat,
            watch_alive,
            cpu: Some(sample.cpu),
            gpu: sample.gpu,
        };
        self.previous = Some((snap, Instant::now()));
        live
    }

    fn profiles(&self) -> Vec<ProfileInfo> {
        self.cfg
            .profiles
            .iter()
            .map(|(n, p)| ProfileInfo {
                name: n.clone(),
                label: p.label.clone(),
                description: p.description.clone(),
            })
            .collect()
    }

    fn set_profile(&mut self, name: &str) -> Result<String, String> {
        let label = self.cfg.profile(name)?.label.clone();
        save_state(&State {
            profile: Some(name.to_string()),
        })?;
        Ok(format!("Profil {label} actif"))
    }

    fn start_watch(&mut self) -> Result<String, String> {
        // La tâche d'ouverture de session si elle existe, sinon un lancement direct.
        if prism_win::install::run_task(prism_core::autostart::ENGINE_TASK).is_ok() {
            return Ok("Prism démarré (tâche planifiée)".into());
        }
        Command::new(prism_exe())
            .args(["watch", "--quiet"])
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok("Prism démarré".into())
    }

    fn rapport(&mut self) -> Result<String, String> {
        let out = hidden(&prism_exe().display().to_string())
            .arg("rapport")
            .output()
            .map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&out.stdout);
        let path = text
            .lines()
            .find_map(|l| l.strip_prefix("Rapport enregistré : "))
            .map(str::trim)
            .ok_or_else(|| format!("rapport impossible : {}", String::from_utf8_lossy(&out.stderr).trim()))?
            .to_string();
        let _ = Command::new("notepad.exe").arg(&path).spawn();
        Ok(format!(
            "Rapport enregistré : {path} (aucune donnée personnelle, à envoyer tel quel)"
        ))
    }

    fn ram_clean(&mut self) -> Result<String, String> {
        let r = prism_core::engine::clean(&mut self.platform, &self.cfg, false)?;
        let freed = match (r.mem_before, r.mem_after) {
            (Some(a), Some(b)) if b.free > a.free => human_bytes(b.free - a.free),
            _ => "0 Mo".into(),
        };
        Ok(format!("{freed} libérés"))
    }

    fn startup(&mut self) -> Result<Vec<StartupRow>, String> {
        let c = StartupCatalog::builtin();
        let mut rows: Vec<StartupRow> = WindowsStartup
            .entries()?
            .into_iter()
            .map(|e| {
                let (advice, why, protected) = match (c.protection(&e), c.advice(&e)) {
                    (Some(reason), _) => ("protégé".to_string(), reason.to_string(), true),
                    (None, Some(r)) => (r.advice.label().to_string(), r.why.clone(), false),
                    (None, None) => ("inconnu".to_string(), e.command.clone(), false),
                };
                StartupRow {
                    source: e.source,
                    enabled: e.enabled(),
                    name: e.name,
                    advice,
                    why,
                    protected,
                }
            })
            .collect();
        rows.sort_by_key(|r| r.name.to_lowercase());
        Ok(rows)
    }

    fn startup_toggle(&mut self, source: Source, name: &str, on: bool) -> Result<String, String> {
        let path = data_dir().join("demarrage.json");
        let mut journal = StartupJournal::load(&path)?;
        let c = StartupCatalog::builtin();
        let mut w = WindowsStartup;
        let e = w
            .entries()?
            .into_iter()
            .find(|e| e.source == source && e.name == name)
            .ok_or("entrée introuvable")?;
        let mut r = StartupReport::default();
        if on {
            demarrage::enable(&mut w, &e, &mut journal, &mut r);
        } else {
            demarrage::disable(&mut w, &c, &e, &mut journal, &mut r);
        }
        journal.save(&path)?;
        startup_summary(&r)
    }

    fn startup_recommended(&mut self) -> Result<String, String> {
        let path = data_dir().join("demarrage.json");
        let mut journal = StartupJournal::load(&path)?;
        let r = demarrage::apply_recommended(&mut WindowsStartup, &StartupCatalog::builtin(), &mut journal)?;
        journal.save(&path)?;
        startup_summary(&r)
    }

    fn startup_restore(&mut self) -> Result<String, String> {
        let path = data_dir().join("demarrage.json");
        let mut journal = StartupJournal::load(&path)?;
        let r = demarrage::restore(&mut WindowsStartup, &mut journal);
        journal.save(&path)?;
        startup_summary(&r)
    }

    fn allege(&mut self) -> Vec<AllegeRow> {
        let c = AllegeCatalog::builtin();
        let mut sys = WindowsSystemConfig;
        let mut rows: Vec<AllegeRow> = c
            .services
            .iter()
            .map(|s| {
                let cur = sys.service_start(&s.name);
                let current = match &cur {
                    Ok(Some(st)) => st.label_fr().to_string(),
                    Ok(None) => "absent".into(),
                    Err(e) => e.clone(),
                };
                let done = matches!(cur, Ok(Some(st)) if st == s.start) || matches!(cur, Ok(None));
                AllegeRow {
                    key: String::new(),
                    by_prism: false,
                    tier: s.tier,
                    label: s.label.clone(),
                    current,
                    target: s.start.label_fr().to_string(),
                    done,
                    why: s.why.clone(),
                }
            })
            .collect();
        rows.extend(c.policies.iter().map(|p| {
            let cur = sys.policy(&p.full_key(), &p.value);
            let current = match &cur {
                Ok(Some(d)) => d.to_string(),
                Ok(None) => "absente".into(),
                Err(e) => e.clone(),
            };
            let done = matches!(&cur, Ok(Some(d)) if *d == p.data);
            AllegeRow {
                key: String::new(),
                by_prism: false,
                tier: p.tier,
                label: p.label.clone(),
                current,
                target: p.data.to_string(),
                done,
                why: p.why.clone(),
            }
        }));
        rows.extend(c.tasks.iter().map(|t| {
            let cur = sys.task_enabled(&t.path);
            let current = match &cur {
                Ok(Some(true)) => "active".into(),
                Ok(Some(false)) => "désactivée".into(),
                Ok(None) => "absente".into(),
                Err(e) => e.clone(),
            };
            let done = matches!(cur, Ok(Some(false)) | Ok(None));
            AllegeRow {
                key: String::new(),
                by_prism: false,
                tier: t.tier,
                label: format!("Tâche : {}", t.label),
                current,
                target: "désactivée".into(),
                done,
                why: t.why.clone(),
            }
        }));
        rows.extend(c.apps.iter().map(|a| {
            let cur = sys.app_installed(&a.package);
            let current = match &cur {
                Ok(true) => "installée".into(),
                Ok(false) => "absente".into(),
                Err(e) => e.clone(),
            };
            AllegeRow {
                key: String::new(),
                by_prism: false,
                tier: a.tier,
                label: format!("Appli : {}", a.label),
                current,
                target: "retirée".into(),
                done: matches!(cur, Ok(false)),
                why: a.why.clone(),
            }
        }));
        // Même ordre que le plan : services, stratégies, tâches, applis.
        let keys = prism_core::allege::plan(&c, &[Tier::Sur, Tier::Avance, Tier::Jeu, Tier::Extreme]);
        debug_assert_eq!(keys.len(), rows.len());
        let journal = allege::AllegeJournal::load(&data_dir().join("allegement.json")).unwrap_or_default();
        for (r, k) in rows.iter_mut().zip(keys) {
            r.key = k.key();
            r.by_prism = allege::journaled(&journal, &r.key);
        }
        rows
    }

    fn game_cfgs(&mut self) -> Vec<crate::backend::GameCfg> {
        use prism_core::jeux::{etat, Reglage};
        let helpers = self.cfg.lists.game_helpers.clone();
        let mut sys = WindowsSystemConfig;
        installed_games()
            .into_iter()
            .map(|g| {
                let exes: Vec<String> =
                    prism_core::jeux::candidate_exes(&prism_win::game_files(&g.install_dir), &helpers)
                        .into_iter()
                        .map(|rel| std::path::Path::new(&g.install_dir).join(rel).display().to_string())
                        .collect();
                crate::backend::GameCfg {
                    gpu: etat(&mut sys, Reglage::Gpu, &exes),
                    plein_ecran: etat(&mut sys, Reglage::PleinEcran, &exes),
                    name: g.name,
                    exes,
                }
            })
            .collect()
    }

    fn game_set(&mut self, name: &str, r: prism_core::jeux::Reglage, on: bool) -> Result<String, String> {
        let cfg = self
            .game_cfgs()
            .into_iter()
            .find(|g| g.name == name)
            .ok_or_else(|| format!("jeu introuvable : {name}"))?;
        let dir = prism_core::paths::user_dir();
        let mut journal = prism_core::jeux::Journal::charger(&dir);
        let n = prism_core::jeux::appliquer(&mut WindowsSystemConfig, &mut journal, r, &cfg.exes, on);
        journal.enregistrer(&dir)?;
        let n = n?;
        Ok(format!(
            "{name} : {} {} ({n} exécutable(s)) — pris en compte au prochain lancement du jeu",
            r.label(),
            if on { "activé" } else { "retiré" }
        ))
    }

    fn last_session(&mut self) -> Option<prism_core::jeux::Partie> {
        prism_core::jeux::Partie::charger(&data_dir())
    }

    fn windows_design_accent_fond(&mut self, accent: bool, fond: bool) -> Result<Vec<String>, String> {
        let palette = self.bar_config().theme.palette();
        prism_win::windesign::apply(&palette, accent, fond, &prism_core::paths::user_dir())
    }

    fn windows_design_restore(&mut self) -> Result<String, String> {
        let mut done = prism_win::windesign::restore(&prism_core::paths::user_dir())?;
        let r = self.appearance_restore()?;
        done.push(r);
        Ok(format!("Windows remis comme avant : {}", done.join(", ")))
    }

    fn services(&mut self) -> Result<Vec<crate::backend::ServiceRow>, String> {
        let c = AllegeCatalog::builtin();
        let journal = allege::AllegeJournal::load(&data_dir().join("allegement.json")).unwrap_or_default();
        Ok(prism_win::services::services_list()?
            .into_iter()
            .map(|info| crate::backend::ServiceRow {
                protected: c.protection(&info.name).map(String::from),
                superflu: allege::superflu(&c, &info.name),
                by_prism: allege::journaled(&journal, &format!("svc:{}", info.name.to_ascii_lowercase())),
                info,
            })
            .collect())
    }

    fn service_set(&mut self, name: &str, to: allege::StartType) -> Result<String, String> {
        let c = AllegeCatalog::builtin();
        let path = data_dir().join("allegement.json");
        let mut journal = allege::AllegeJournal::load(&path)?;
        allege::set_service(&mut WindowsSystemConfig, &c, &mut journal, name, to, &mut |j| {
            j.save(&path)
        })
    }

    fn service_restore(&mut self, name: &str) -> Result<String, String> {
        self.allege_toggle(&format!("svc:{}", name.to_ascii_lowercase()), false)
    }

    fn allege_toggle(&mut self, key: &str, on: bool) -> Result<String, String> {
        let c = AllegeCatalog::builtin();
        let path = data_dir().join("allegement.json");
        let mut journal = allege::AllegeJournal::load(&path)?;
        let r = if on {
            let change = allege::change_for(&c, key).ok_or_else(|| format!("élément inconnu : {key}"))?;
            allege::apply(&mut WindowsSystemConfig, &c, &[change], &mut journal, &mut |j| {
                j.save(&path)
            })
        } else {
            if !allege::journaled(&journal, key) {
                return Err("Déjà ainsi avant Prism : rien à remettre".into());
            }
            let r = allege::restore_keys(&mut WindowsSystemConfig, &mut journal, &[key.to_string()]);
            journal.save(&path)?;
            r
        };
        match (r.done.first(), r.failed.first()) {
            (_, Some(f)) => Err(f.clone()),
            (Some(d), None) => Ok(d.clone()),
            (None, None) => Ok(r.unchanged.first().cloned().unwrap_or_else(|| "Rien à changer".into())),
        }
    }

    fn allege_apply(&mut self, tier: Tier) -> Result<String, String> {
        let c = AllegeCatalog::builtin();
        let path = data_dir().join("allegement.json");
        let mut journal = allege::AllegeJournal::load(&path)?;
        let r = allege::apply(
            &mut WindowsSystemConfig,
            &c,
            &allege::plan(&c, &[tier]),
            &mut journal,
            &mut |j| j.save(&path),
        );
        if r.failed.is_empty() {
            Ok(format!(
                "{} appliqué(s), {} déjà fait(s)",
                r.done.len(),
                r.unchanged.len()
            ))
        } else {
            Err(format!("{} échec(s) : {}", r.failed.len(), r.failed.join(" · ")))
        }
    }

    fn allege_restore(&mut self) -> Result<String, String> {
        let path = data_dir().join("allegement.json");
        let mut journal = allege::AllegeJournal::load(&path)?;
        let r = allege::restore(&mut WindowsSystemConfig, &mut journal);
        journal.save(&path)?;
        if r.failed.is_empty() {
            Ok(format!("{} valeur(s) d'origine remise(s)", r.done.len()))
        } else {
            Err(r.failed.join(" · "))
        }
    }

    fn privacy(
        &mut self,
    ) -> (
        Vec<prism_core::privacy::Row>,
        Vec<prism_core::privacy::TelemetryConnection>,
    ) {
        let c = prism_core::privacy::Catalog::builtin();
        let rows = prism_core::privacy::status(&mut prism_win::WindowsPrivacy::new(), &c);
        (rows, prism_win::telemetry_connections(&c))
    }

    fn privacy_toggle(&mut self, key: &str, on: bool) -> Result<String, String> {
        use prism_core::privacy::{apply, change_for, journal_path, journaled, restore_keys, Catalog, Journal};
        let path = journal_path();
        let mut journal = Journal::load(&path)?;
        let c = Catalog::builtin();
        let r = if on {
            let change = change_for(&c, key).ok_or_else(|| format!("élément inconnu : {key}"))?;
            apply(
                &mut prism_win::WindowsPrivacy::new(),
                &[change],
                &mut journal,
                &mut |j| j.save(&path),
            )
        } else {
            if !journaled(&journal, key) {
                return Err("Déjà ainsi avant Prism : rien à remettre".into());
            }
            restore_keys(&mut prism_win::WindowsPrivacy::new(), &mut journal, &[key.to_string()])
        };
        journal.save(&path)?;
        match (r.done.first(), r.failed.first()) {
            (_, Some(f)) => Err(f.clone()),
            (Some(d), None) => Ok(d.clone()),
            (None, None) => Ok(r.unchanged.first().cloned().unwrap_or_else(|| "Rien à changer".into())),
        }
    }

    fn privacy_apply(&mut self, level: prism_core::privacy::Level) -> Result<String, String> {
        use prism_core::privacy::{apply, journal_path, plan, Catalog, Journal};
        let c = Catalog::builtin();
        let path = journal_path();
        let mut journal = Journal::load(&path)?;
        let r = apply(
            &mut prism_win::WindowsPrivacy::new(),
            &plan(&c, level),
            &mut journal,
            &mut |j| j.save(&path),
        );
        if r.failed.is_empty() {
            Ok(format!(
                "Niveau « {} » : {} appliqué(s), {} déjà fait(s) ou absent(s)",
                level.label(),
                r.done.len(),
                r.unchanged.len()
            ))
        } else {
            Err(format!("{} échec(s) : {}", r.failed.len(), r.failed.join(" · ")))
        }
    }

    fn privacy_restore(&mut self) -> Result<String, String> {
        use prism_core::privacy::{journal_path, restore, Journal};
        let path = journal_path();
        let mut journal = Journal::load(&path)?;
        let r = restore(&mut prism_win::WindowsPrivacy::new(), &mut journal);
        journal.save(&path)?;
        if r.failed.is_empty() {
            Ok(format!("{} élément(s) remis comme avant", r.done.len()))
        } else {
            Err(r.failed.join(" · "))
        }
    }

    fn games(&mut self) -> Vec<Game> {
        installed_games()
    }

    fn launch(&mut self, game: &Game) -> Result<String, String> {
        match &game.launch {
            Launch::Uri(u) => Command::new("explorer.exe").arg(u).spawn().map(|_| ()),
            Launch::Exe(e) => Command::new(e).spawn().map(|_| ()),
            Launch::None => return Err(format!("{} : lancez-le depuis {}", game.name, game.store.label())),
        }
        .map_err(|e| e.to_string())?;
        Ok(format!("Lancement de {}", game.name))
    }

    fn appearance(&mut self) -> Vec<crate::backend::AppearanceRow> {
        crate::appearance_common::rows(&mut prism_win::WindowsAppearance)
    }

    fn appearance_presets(&self) -> Vec<crate::backend::PresetInfo> {
        crate::appearance_common::presets()
    }

    fn appearance_set(&mut self, id: &str, option: usize) -> Result<String, String> {
        let path = prism_core::paths::user_dir().join("apparence.json");
        let mut j = prism_core::apparence::AppearanceJournal::load(&path)?;
        let r = crate::appearance_common::set(&mut prism_win::WindowsAppearance, &mut j, id, option);
        j.save(&path)?;
        r
    }

    fn appearance_preset(&mut self, id: &str) -> Result<String, String> {
        let path = prism_core::paths::user_dir().join("apparence.json");
        let mut j = prism_core::apparence::AppearanceJournal::load(&path)?;
        let r = crate::appearance_common::preset(&mut prism_win::WindowsAppearance, &mut j, id);
        j.save(&path)?;
        r
    }

    fn appearance_restore(&mut self) -> Result<String, String> {
        let path = prism_core::paths::user_dir().join("apparence.json");
        let mut j = prism_core::apparence::AppearanceJournal::load(&path)?;
        let r = crate::appearance_common::restore_all(&mut prism_win::WindowsAppearance, &mut j);
        j.save(&path)?;
        r
    }

    fn bar_config(&mut self) -> prism_core::bar::BarConfig {
        prism_core::bar::BarConfig::load()
    }

    fn set_bar_config(&mut self, cfg: &prism_core::bar::BarConfig) -> Result<(), String> {
        cfg.save()
    }

    fn bar_running(&mut self) -> bool {
        prism_win::bar_app::running()
    }

    fn autostart(&mut self) -> bool {
        prism_win::install::autostart_enabled()
    }

    fn set_autostart(&mut self, on: bool) -> Result<String, String> {
        if on {
            let dir = prism_exe()
                .parent()
                .map(|d| d.display().to_string())
                .ok_or("dossier d'installation introuvable")?;
            prism_win::install::autostart_install(&dir)?;
            // Le moteur part tout de suite (sous le compte système, comme au démarrage).
            let _ = prism_win::install::run_task(prism_core::autostart::ENGINE_TASK);
            Ok(
                "Prism démarrera avec Windows : moteur avant même l'écran de connexion, barre à l'ouverture de session"
                    .into(),
            )
        } else {
            prism_win::install::autostart_remove();
            Ok("Démarrage avec Windows désactivé".into())
        }
    }

    fn uninstall(&mut self) -> Result<String, String> {
        // Le raccourci « Désinstaller Prism » posé par l'installateur (msiexec /x).
        let lnk = std::path::PathBuf::from(std::env::var("ProgramData").unwrap_or_else(|_| "C:\\ProgramData".into()))
            .join("Microsoft\\Windows\\Start Menu\\Programs\\Désinstaller Prism.lnk");
        if !lnk.exists() {
            return Err("Prism n'a pas été installé avec l'installateur : désinstallez-le depuis Paramètres > Applications, ou lancez « prism desinstaller ».".into());
        }
        Command::new("cmd.exe")
            .args(["/c", "start", ""])
            .arg(&lnk)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok("Désinstallation lancée : confirmez dans la fenêtre de Windows".into())
    }

    fn welcome_done(&mut self) -> bool {
        prism_core::paths::user_dir().join("accueil-vu").exists()
    }

    fn set_welcome_done(&mut self) {
        let _ = std::fs::create_dir_all(prism_core::paths::user_dir());
        let _ = std::fs::write(prism_core::paths::user_dir().join("accueil-vu"), b"1");
    }

    fn bar_start(&mut self) -> Result<String, String> {
        let exe = prism_exe().with_file_name("prism-bar.exe");
        Command::new(&exe)
            .spawn()
            .map_err(|e| format!("{} : {e}", exe.display()))?;
        Ok("Prism Bar lancée".into())
    }

    fn bar_stop(&mut self) -> Result<String, String> {
        if prism_win::bar_app::stop() {
            Ok("Prism Bar arrêtée, barre Windows remise".into())
        } else {
            Err("Prism Bar ne tournait pas".into())
        }
    }

    fn packs(&self) -> Vec<PackInfo> {
        self.cfg
            .packs
            .iter()
            .map(|(id, p)| PackInfo {
                id: id.clone(),
                label: p.label.clone(),
                tools: prism_core::tools::resolve_pack(&self.cfg, id)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|t| (t.id, t.name, t.reason))
                    .collect(),
            })
            .collect()
    }

    fn tools_scan(&mut self) {
        let state = self.tools.clone();
        {
            let Ok(mut s) = state.lock() else { return };
            if s.scanning {
                return;
            }
            s.scanning = true;
        }
        let cfg = self.cfg.clone();
        std::thread::spawn(move || {
            let installed = scan_tools(&cfg);
            if let Ok(mut s) = state.lock() {
                s.installed = installed;
                s.scanning = false;
                s.scanned = true;
            }
        });
    }

    fn tools_view(&mut self) -> crate::backend::ToolsView {
        self.tools.lock().map(|s| s.clone()).unwrap_or_default()
    }

    fn tools_run(&mut self, install: bool, ids: Vec<String>) -> Result<String, String> {
        use crate::backend::ToolJob;
        let tools = if install {
            // Dépendances d'abord ; celles déjà installées sont sautées.
            let have = self.tools_view().installed;
            prism_core::tools::resolve_tools(&self.cfg, &ids)?
                .into_iter()
                .filter(|t| ids.contains(&t.id) || !have.contains(&t.id))
                .collect::<Vec<_>>()
        } else {
            ids.iter().filter_map(|id| self.cfg.tool(id).cloned()).collect()
        };
        {
            let mut s = self.tools.lock().map_err(|_| "verrou".to_string())?;
            if s.job.as_ref().is_some_and(|j| !j.done) {
                return Err("Une installation est déjà en cours".into());
            }
            s.job = Some(ToolJob {
                install,
                total: tools.len(),
                ..Default::default()
            });
        }
        let state = self.tools.clone();
        let cfg = self.cfg.clone();
        let count = tools.len();
        std::thread::spawn(move || {
            for (i, t) in tools.iter().enumerate() {
                if let Ok(mut s) = state.lock() {
                    if let Some(j) = s.job.as_mut() {
                        j.current = t.name.clone();
                        j.step = i + 1;
                    }
                }
                let cmds = if install {
                    prism_core::tools::install_commands(t)
                } else {
                    prism_core::tools::uninstall_commands(t)
                };
                let mut error = None;
                for cmd in &cmds {
                    match hidden(&cmd[0]).args(&cmd[1..]).output() {
                        Ok(o) if o.status.success() => {}
                        Ok(o) => {
                            let out = String::from_utf8_lossy(&o.stdout);
                            let last = out.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
                            error = Some(format!("{} : échec (code {:?}) {last}", t.name, o.status.code()));
                            break;
                        }
                        Err(e) => {
                            error = Some(format!("{} : {} introuvable ({e})", t.name, cmd[0]));
                            break;
                        }
                    }
                }
                if let Ok(mut s) = state.lock() {
                    if let Some(j) = s.job.as_mut() {
                        match error {
                            Some(e) => j.errors.push(e),
                            None => j.ok.push(t.name.clone()),
                        }
                    }
                }
            }
            let installed = scan_tools(&cfg);
            if let Ok(mut s) = state.lock() {
                s.installed = installed;
                s.scanned = true;
                if let Some(j) = s.job.as_mut() {
                    j.done = true;
                }
            }
        });
        Ok(format!(
            "{} en cours ({} outil(s)) : vous pouvez continuer à utiliser Prism",
            if install { "Installation" } else { "Désinstallation" },
            count
        ))
    }

    fn install_pack(&mut self, pack: &str) -> Result<String, String> {
        let ids = self.cfg.packs.get(pack).map(|p| p.tools.clone()).unwrap_or_default();
        self.tools_run(true, ids)
    }
}
