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
}

impl WinBackend {
    pub fn new() -> WinBackend {
        WinBackend {
            cfg: load_config().unwrap_or_else(|_| Config::builtin()),
            platform: WindowsPlatform::new(),
            previous: None,
        }
    }
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
        let watch_alive = etat.as_ref().is_some_and(|e| e.alive(now_unix(), 15));
        let cores = prism_core::cores::split(&snap.cpus)
            .map(|s| s.describe())
            .unwrap_or_else(|| format!("{} cœurs logiques homogènes", snap.cpus.len()));
        let live = Live {
            profile: active_profile(&self.cfg),
            mem: snap.mem,
            top,
            cores,
            etat,
            watch_alive,
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
        let task = Command::new("schtasks.exe").args(["/Run", "/TN", "Prism OS"]).output();
        if task.map(|o| o.status.success()).unwrap_or(false) {
            return Ok("Prism démarré (tâche planifiée)".into());
        }
        Command::new(prism_exe())
            .args(["watch", "--quiet"])
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok("Prism démarré".into())
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
                tier: t.tier,
                label: format!("Tâche : {}", t.label),
                current,
                target: "désactivée".into(),
                done,
                why: t.why.clone(),
            }
        }));
        rows
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
        let path = data_dir().join("apparence.json");
        let mut j = prism_core::apparence::AppearanceJournal::load(&path)?;
        let r = crate::appearance_common::set(&mut prism_win::WindowsAppearance, &mut j, id, option);
        j.save(&path)?;
        r
    }

    fn appearance_preset(&mut self, id: &str) -> Result<String, String> {
        let path = data_dir().join("apparence.json");
        let mut j = prism_core::apparence::AppearanceJournal::load(&path)?;
        let r = crate::appearance_common::preset(&mut prism_win::WindowsAppearance, &mut j, id);
        j.save(&path)?;
        r
    }

    fn appearance_restore(&mut self) -> Result<String, String> {
        let path = data_dir().join("apparence.json");
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
                    .map(|t| (t.name, t.reason))
                    .collect(),
            })
            .collect()
    }

    fn install_pack(&mut self, pack: &str) -> Result<String, String> {
        // Une console visible : l'installation montre sa progression et ses questions.
        let exe = prism_exe();
        Command::new("cmd.exe")
            .args(["/c", "start", "Prism - outils"])
            .arg(exe)
            .args(["tools", "install", pack])
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(format!("Installation du pack {pack} lancée dans une console"))
    }
}
