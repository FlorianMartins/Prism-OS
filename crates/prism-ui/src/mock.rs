//! Backend simulé : un PC de joueur crédible, pour les tests et les captures.

use prism_core::allege::{Catalog, Tier};
use prism_core::config::Config;
use prism_core::demarrage::Source;
use prism_core::etat::Etat;
use prism_core::library::{Game, Launch, Store};
use prism_core::model::MemStatus;

use crate::backend::{AllegeRow, Backend, Live, PackInfo, ProfileInfo, StartupRow, TopProc};

const GIB: u64 = 1024 * 1024 * 1024;
const MIB: u64 = 1024 * 1024;

pub struct MockBackend {
    pub bar: prism_core::bar::BarConfig,
    pub bar_on: bool,
    pub appearance: prism_core::apparence::MockAppearance,
    pub appearance_journal: prism_core::apparence::AppearanceJournal,
    pub profile: String,
    pub startup: Vec<StartupRow>,
    pub applied: Vec<Tier>,
    pub in_game: bool,
    pub log: Vec<String>,
    pub privacy: prism_core::privacy::MockPrivacy,
    pub privacy_journal: prism_core::privacy::Journal,
    pub welcome_seen: bool,
    pub autostart_on: bool,
    pub noyau: prism_core::noyau::Reglages,
    pub webview: prism_core::webview::Reglages,
    pub tools_installed: Vec<String>,
    pub ram_auto: prism_core::ram_auto::Reglages,
}

impl Default for MockBackend {
    fn default() -> Self {
        let row = |source, name: &str, enabled, advice: &str, why: &str, protected| StartupRow {
            source,
            name: name.into(),
            enabled,
            advice: advice.into(),
            why: why.into(),
            protected,
        };
        MockBackend {
            bar: prism_core::bar::BarConfig {
                opacity_rules: vec![prism_core::bar::OpacityRule {
                    process: "windowsterminal.exe".into(),
                    opacity: 90,
                }],
                ..Default::default()
            },
            bar_on: true,
            appearance: prism_core::apparence::MockAppearance::factory(&prism_core::apparence::Catalog::builtin()),
            appearance_journal: Default::default(),
            profile: "gaming".into(),
            startup: vec![
                row(
                    Source::UserRun,
                    "Discord",
                    true,
                    "optionnel",
                    "Discord au démarrage : pratique pour être joignable, sinon il se lance en quelques secondes.",
                    false,
                ),
                row(
                    Source::UserRun,
                    "MicrosoftEdgeAutoLaunch",
                    true,
                    "à désactiver",
                    "Précharge Edge à l'ouverture de session : des processus cachés en permanence.",
                    false,
                ),
                row(
                    Source::UserRun,
                    "OneDrive",
                    true,
                    "optionnel",
                    "Sans OneDrive au démarrage, vos dossiers ne se synchronisent plus tant que vous ne l'ouvrez pas.",
                    false,
                ),
                row(
                    Source::MachineRun,
                    "Riot Vanguard",
                    true,
                    "protégé",
                    "Anti-cheat : son absence au démarrage peut empêcher les jeux de se lancer.",
                    true,
                ),
                row(
                    Source::MachineRun,
                    "SecurityHealth",
                    true,
                    "protégé",
                    "Sécurité Windows",
                    true,
                ),
                row(
                    Source::UserRun,
                    "Spotify",
                    false,
                    "à désactiver",
                    "Spotify n'a pas besoin de tourner avant que vous lanciez la musique.",
                    false,
                ),
                row(
                    Source::UserRun,
                    "Steam",
                    true,
                    "optionnel",
                    "Steam au démarrage : amis en ligne et mises à jour de jeux en arrière-plan.",
                    false,
                ),
                row(
                    Source::StoreTask,
                    r"MSTeams_8wekyb3d8bbwe\TeamsTfwStartupTask",
                    true,
                    "à désactiver",
                    "Teams démarre tout seul et reste en mémoire.",
                    false,
                ),
            ],
            applied: vec![Tier::Sur],
            in_game: false,
            privacy: {
                // Machine simulée : l'allègement « sûr » est déjà en place.
                let mut p = prism_core::privacy::MockPrivacy::default();
                p.services
                    .insert("DiagTrack".into(), prism_core::allege::StartType::Disabled);
                p
            },
            privacy_journal: prism_core::privacy::Journal::default(),
            welcome_seen: false,
            autostart_on: false,
            noyau: Default::default(),
            webview: Default::default(),
            tools_installed: vec!["wireshark".into(), "x64dbg".into()],
            ram_auto: Default::default(),
            log: Vec::new(),
        }
    }
}

impl Backend for MockBackend {
    fn elevated(&self) -> bool {
        true
    }

    fn relaunch_elevated(&mut self) -> Result<String, String> {
        Ok("déjà administrateur".into())
    }

    fn live(&mut self) -> Live {
        let p = |name: &str, cpu: f32, ram_mb: u64, class: &str| TopProc {
            name: name.into(),
            cpu_percent: cpu,
            ram: ram_mb * MIB,
            class: class.into(),
        };
        let mut top = vec![
            p("chrome.exe", 6.2, 1840, "arrière-plan"),
            p("discord.exe", 1.4, 420, "compagnon"),
            p("dwm.exe", 1.1, 96, "protégé"),
            p("steamwebhelper.exe", 0.6, 310, "compagnon"),
            p("winword.exe", 0.0, 612, "arrière-plan"),
            p("onedrive.exe", 0.0, 84, "arrière-plan"),
        ];
        if self.in_game {
            top.insert(0, p("eldenring.exe", 38.5, 7200, "jeu"));
        }
        Live {
            profile: self.profile.clone(),
            mem: MemStatus {
                total: 32 * GIB,
                free: 14 * GIB,
                standby_low: 600 * MIB,
                standby_total: 9 * GIB,
            },
            top,
            cores: "Ryzen X3D : arrière-plan sur la puce sans cache 3D (16 cœurs logiques), 16 laissés au jeu".into(),
            etat: Some(Etat {
                updated_unix: 0,
                profile: self.profile.clone(),
                game: if self.in_game {
                    vec!["eldenring.exe".into()]
                } else {
                    vec![]
                },
                eased: vec![
                    "onedrive.exe".into(),
                    "spotify.exe".into(),
                    "widgetboard.exe".into(),
                    "winword.exe".into(),
                ],
                mem: MemStatus::default(),
                cores: None,
                recent: vec![
                    "18:02:11 Quotidien : winword.exe : EcoQoS activé".into(),
                    "18:02:11 Quotidien : winword.exe : priorité mémoire -> Low".into(),
                    "18:27:40 Quotidien : winword.exe : mémoire de travail rognée".into(),
                    "18:31:05 Mode Jeu : eldenring.exe".into(),
                    "19:44:52 Fin du Mode Jeu, réglages restaurés".into(),
                ],
            }),
            watch_alive: true,
            cpu: Some(23.0),
            gpu: Some(41.0),
        }
    }

    fn profiles(&self) -> Vec<ProfileInfo> {
        Config::builtin()
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
        self.profile = name.into();
        Ok(format!("Profil {name} actif"))
    }

    fn start_watch(&mut self) -> Result<String, String> {
        Ok("Prism démarré".into())
    }

    fn rapport(&mut self) -> Result<String, String> {
        Ok("Rapport enregistré sur le Bureau".into())
    }
    fn ram_clean(&mut self) -> Result<String, String> {
        Ok("2.9 Go libérés".into())
    }

    fn startup(&mut self) -> Result<Vec<StartupRow>, String> {
        Ok(self.startup.clone())
    }

    fn startup_toggle(&mut self, source: Source, name: &str, on: bool) -> Result<String, String> {
        let r = self
            .startup
            .iter_mut()
            .find(|r| r.source == source && r.name == name)
            .ok_or("absent")?;
        if r.protected {
            return Err(format!("{name} est protégé"));
        }
        r.enabled = on;
        Ok(format!("{name} {}", if on { "activé" } else { "désactivé" }))
    }

    fn startup_recommended(&mut self) -> Result<String, String> {
        let mut n = 0;
        for r in self
            .startup
            .iter_mut()
            .filter(|r| r.advice == "à désactiver" && r.enabled)
        {
            r.enabled = false;
            n += 1;
        }
        Ok(format!("{n} appli(s) désactivée(s)"))
    }

    fn startup_restore(&mut self) -> Result<String, String> {
        *self = MockBackend {
            profile: self.profile.clone(),
            ..Default::default()
        };
        Ok("Démarrage restauré".into())
    }

    fn allege(&mut self) -> Vec<AllegeRow> {
        let c = Catalog::builtin();
        let mut rows: Vec<AllegeRow> = c
            .services
            .iter()
            .map(|s| AllegeRow {
                key: String::new(),
                by_prism: false,
                tier: s.tier,
                label: s.label.clone(),
                current: if self.applied.contains(&s.tier) {
                    format!("{:?}", s.start)
                } else {
                    "Auto".into()
                },
                target: s.start.label_fr().into(),
                done: self.applied.contains(&s.tier),
                why: s.why.clone(),
            })
            .collect();
        rows.extend(c.policies.iter().map(|p| AllegeRow {
            key: String::new(),
            by_prism: false,
            tier: p.tier,
            label: p.label.clone(),
            current: if self.applied.contains(&p.tier) {
                p.data.to_string()
            } else {
                "absente".into()
            },
            target: p.data.to_string(),
            done: self.applied.contains(&p.tier),
            why: p.why.clone(),
        }));
        rows.extend(c.tasks.iter().map(|t| AllegeRow {
            key: String::new(),
            by_prism: false,
            tier: t.tier,
            label: format!("Tâche : {}", t.label),
            current: if self.applied.contains(&t.tier) {
                "désactivée".into()
            } else {
                "active".into()
            },
            target: "désactivée".into(),
            done: self.applied.contains(&t.tier),
            why: t.why.clone(),
        }));
        rows.extend(c.apps.iter().map(|a| {
            AllegeRow {
                key: String::new(),
                by_prism: false,
                tier: a.tier,
                label: format!("Appli : {}", a.label),
                current: if self.applied.contains(&a.tier) {
                    "absente"
                } else {
                    "installée"
                }
                .into(),
                target: "retirée".into(),
                done: self.applied.contains(&a.tier),
                why: a.why.clone(),
            }
        }));
        // Même ordre que le plan : services, stratégies, tâches, applis.
        let keys = prism_core::allege::plan(&c, &[Tier::Sur, Tier::Avance, Tier::Jeu, Tier::Extreme]);
        debug_assert_eq!(keys.len(), rows.len());
        for (r, k) in rows.iter_mut().zip(keys) {
            r.key = k.key();
            r.by_prism = r.done;
        }
        rows
    }

    fn ram_auto(&mut self) -> prism_core::ram_auto::Reglages {
        self.ram_auto.clone()
    }
    fn set_ram_auto(&mut self, r: &prism_core::ram_auto::Reglages) -> Result<String, String> {
        self.ram_auto = r.clone();
        Ok("ok".into())
    }
    fn webview(&mut self) -> prism_core::webview::Reglages {
        self.webview.clone()
    }
    fn set_webview(&mut self, r: &prism_core::webview::Reglages) -> Result<String, String> {
        self.webview = r.clone();
        Ok("WebView en arrière-plan : réglage enregistré".into())
    }
    fn noyau(&mut self) -> prism_core::noyau::Reglages {
        self.noyau.clone()
    }
    fn set_noyau(&mut self, r: &prism_core::noyau::Reglages) -> Result<String, String> {
        self.noyau = r.clone();
        Ok("Plan « jeu noyau » enregistré".into())
    }

    fn game_cfgs(&mut self) -> Vec<crate::backend::GameCfg> {
        self.games()
            .into_iter()
            .enumerate()
            .map(|(i, g)| crate::backend::GameCfg {
                name: g.name,
                exes: if i == 2 {
                    Vec::new()
                } else {
                    vec![format!("{}\\game.exe", g.install_dir)]
                },
                gpu: if i == 2 { None } else { Some(i == 0) },
                plein_ecran: if i == 2 { None } else { Some(false) },
            })
            .collect()
    }

    fn game_set(&mut self, name: &str, r: prism_core::jeux::Reglage, on: bool) -> Result<String, String> {
        Ok(format!(
            "{name} : {} {}",
            r.label(),
            if on { "activé" } else { "retiré" }
        ))
    }

    fn last_session(&mut self) -> Option<prism_core::jeux::Partie> {
        Some(prism_core::jeux::Partie {
            jeux: vec!["cs2.exe".into()],
            debut: "2026-10-04 19:12 UTC".into(),
            duree_secs: 5_460,
            actions: 23,
            dispo_debut: 9 << 30,
            dispo_min: 7 << 30,
            anticheat: None,
        })
    }

    fn services(&mut self) -> Result<Vec<crate::backend::ServiceRow>, String> {
        use prism_core::allege::{ServiceInfo, StartType};
        let c = Catalog::builtin();
        let row = |name: &str, display: &str, start, running| crate::backend::ServiceRow {
            protected: c.protection(name).map(String::from),
            by_prism: name == "SysMain",
            superflu: prism_core::allege::superflu(&c, name),
            info: ServiceInfo {
                name: name.into(),
                display: display.into(),
                start: Some(start),
                running,
                per_user: false,
            },
        };
        Ok(vec![
            row(
                "BITS",
                "Service de transfert intelligent en arrière-plan",
                StartType::Manual,
                true,
            ),
            row(
                "DiagTrack",
                "Expériences des utilisateurs connectés et télémétrie",
                StartType::Disabled,
                false,
            ),
            row("Spooler", "Spouleur d'impression", StartType::Auto, true),
            row("SysMain", "SysMain", StartType::Disabled, false),
            row("vgc", "Vanguard (Riot)", StartType::Manual, false),
            row("WSearch", "Windows Search", StartType::AutoDelayed, true),
            row("wuauserv", "Windows Update", StartType::Manual, true),
        ])
    }

    fn service_set(&mut self, name: &str, to: prism_core::allege::StartType) -> Result<String, String> {
        Ok(format!("{name} : {}", to.label_fr()))
    }

    fn service_restore(&mut self, name: &str) -> Result<String, String> {
        Ok(format!("{name} : origine remise"))
    }

    fn compression(&mut self) -> Option<bool> {
        Some(true)
    }

    fn allege_toggle(&mut self, key: &str, on: bool) -> Result<String, String> {
        Ok(format!("{key} {}", if on { "appliqué" } else { "remis" }))
    }

    fn allege_apply(&mut self, tier: Tier) -> Result<String, String> {
        if !self.applied.contains(&tier) {
            self.applied.push(tier);
        }
        Ok(format!("Niveau {} appliqué", tier.label()))
    }

    fn privacy(
        &mut self,
    ) -> (
        Vec<prism_core::privacy::Row>,
        Vec<prism_core::privacy::TelemetryConnection>,
    ) {
        let c = prism_core::privacy::Catalog::builtin();
        let rows = prism_core::privacy::status(&mut self.privacy, &c);
        // Sans règle de pare-feu, la simulation montre la télémétrie qui parle.
        let conns = if self.privacy.rules.is_empty() {
            vec![
                prism_core::privacy::TelemetryConnection {
                    component: "service DiagTrack".into(),
                    remote: "20.42.65.92:443".into(),
                    state: "établie".into(),
                },
                prism_core::privacy::TelemetryConnection {
                    component: "CompatTelRunner.exe".into(),
                    remote: "13.89.179.10:443".into(),
                    state: "en attente de réponse".into(),
                },
            ]
        } else {
            Vec::new()
        };
        (rows, conns)
    }

    fn privacy_toggle(&mut self, key: &str, on: bool) -> Result<String, String> {
        use prism_core::privacy::{apply, change_for, journaled, restore_keys, Catalog};
        let mut journal = std::mem::take(&mut self.privacy_journal);
        let c = Catalog::builtin();
        let r = if on {
            let change = change_for(&c, key).ok_or_else(|| format!("élément inconnu : {key}"))?;
            apply(&mut self.privacy, &[change], &mut journal, &mut |_| Ok(()))
        } else {
            if !journaled(&journal, key) {
                self.privacy_journal = journal;
                return Err("Déjà ainsi avant Prism : rien à remettre".into());
            }
            restore_keys(&mut self.privacy, &mut journal, &[key.to_string()])
        };
        self.privacy_journal = journal;
        match (r.done.first(), r.failed.first()) {
            (_, Some(f)) => Err(f.clone()),
            (Some(d), None) => Ok(d.clone()),
            (None, None) => Ok(r.unchanged.first().cloned().unwrap_or_else(|| "Rien à changer".into())),
        }
    }

    fn privacy_apply(&mut self, level: prism_core::privacy::Level) -> Result<String, String> {
        let c = prism_core::privacy::Catalog::builtin();
        let r = prism_core::privacy::apply(
            &mut self.privacy,
            &prism_core::privacy::plan(&c, level),
            &mut self.privacy_journal,
            &mut |_| Ok(()),
        );
        Ok(format!("Niveau « {} » : {} appliqué(s)", level.label(), r.done.len()))
    }

    fn privacy_restore(&mut self) -> Result<String, String> {
        let r = prism_core::privacy::restore(&mut self.privacy, &mut self.privacy_journal);
        Ok(format!("{} élément(s) remis comme avant", r.done.len()))
    }

    fn allege_restore(&mut self) -> Result<String, String> {
        self.applied.clear();
        Ok("Valeurs d'origine remises".into())
    }

    fn games(&mut self) -> Vec<Game> {
        let g = |name: &str, store, dir: &str| Game {
            name: name.into(),
            store,
            install_dir: dir.into(),
            launch: Launch::Uri("steam://rungameid/1".into()),
        };
        vec![
            g(
                "Baldur's Gate 3",
                Store::Steam,
                r"D:\SteamLibrary\steamapps\common\Baldurs Gate 3",
            ),
            g(
                "Counter-Strike 2",
                Store::Steam,
                r"C:\Steam\steamapps\common\Counter-Strike Global Offensive",
            ),
            g("Cyberpunk 2077", Store::Gog, r"D:\GOG Games\Cyberpunk 2077"),
            g(
                "ELDEN RING",
                Store::Steam,
                r"D:\SteamLibrary\steamapps\common\ELDEN RING",
            ),
            g("Fortnite", Store::Epic, r"C:\Program Files\Epic Games\Fortnite"),
            g("Overwatch", Store::BattleNet, r"C:\Program Files (x86)\Overwatch"),
            g(
                "Rocket League",
                Store::Epic,
                r"C:\Program Files\Epic Games\rocketleague",
            ),
            g("The Witcher 3", Store::Gog, r"D:\GOG Games\The Witcher 3"),
        ]
    }

    fn launch(&mut self, game: &Game) -> Result<String, String> {
        self.log.push(format!("launch {}", game.name));
        self.in_game = true;
        Ok(format!("Lancement de {}", game.name))
    }

    fn appearance(&mut self) -> Vec<crate::backend::AppearanceRow> {
        crate::appearance_common::rows(&mut self.appearance)
    }

    fn appearance_presets(&self) -> Vec<crate::backend::PresetInfo> {
        crate::appearance_common::presets()
    }

    fn appearance_set(&mut self, id: &str, option: usize) -> Result<String, String> {
        crate::appearance_common::set(&mut self.appearance, &mut self.appearance_journal, id, option)
    }

    fn appearance_preset(&mut self, id: &str) -> Result<String, String> {
        crate::appearance_common::preset(&mut self.appearance, &mut self.appearance_journal, id)
    }

    fn appearance_restore(&mut self) -> Result<String, String> {
        crate::appearance_common::restore_all(&mut self.appearance, &mut self.appearance_journal)
    }

    fn bar_config(&mut self) -> prism_core::bar::BarConfig {
        self.bar.clone()
    }

    fn set_bar_config(&mut self, cfg: &prism_core::bar::BarConfig) -> Result<(), String> {
        self.bar = cfg.clone().sanitized();
        Ok(())
    }

    fn bar_running(&mut self) -> bool {
        self.bar_on
    }

    fn autostart(&mut self) -> bool {
        self.autostart_on
    }

    fn set_autostart(&mut self, on: bool) -> Result<String, String> {
        self.autostart_on = on;
        Ok("ok".into())
    }

    fn uninstall(&mut self) -> Result<String, String> {
        Ok("Désinstallation lancée (simulation)".into())
    }

    fn welcome_done(&mut self) -> bool {
        self.welcome_seen
    }

    fn set_welcome_done(&mut self) {
        self.welcome_seen = true;
    }

    fn bar_start(&mut self) -> Result<String, String> {
        self.bar_on = true;
        Ok("Prism Bar lancée".into())
    }

    fn bar_stop(&mut self) -> Result<String, String> {
        self.bar_on = false;
        Ok("Prism Bar arrêtée".into())
    }

    fn packs(&self) -> Vec<PackInfo> {
        let cfg = Config::builtin();
        cfg.packs
            .iter()
            .map(|(id, p)| PackInfo {
                id: id.clone(),
                label: p.label.clone(),
                tools: prism_core::tools::resolve_pack(&cfg, id)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|t| (t.id, t.name, t.reason))
                    .collect(),
            })
            .collect()
    }

    fn tools_scan(&mut self) {}
    fn tools_view(&mut self) -> crate::backend::ToolsView {
        crate::backend::ToolsView {
            installed: self.tools_installed.clone(),
            scanned: true,
            ..Default::default()
        }
    }
    fn tools_run(&mut self, install: bool, ids: Vec<String>) -> Result<String, String> {
        for id in &ids {
            if install {
                if !self.tools_installed.contains(id) {
                    self.tools_installed.push(id.clone());
                }
            } else {
                self.tools_installed.retain(|x| x != id);
            }
        }
        Ok(format!(
            "{} outil(s) {}",
            ids.len(),
            if install { "installé(s)" } else { "désinstallé(s)" }
        ))
    }

    fn install_pack(&mut self, pack: &str) -> Result<String, String> {
        Ok(format!("Installation du pack {pack} lancée"))
    }
}
