//! Configuration (TOML) : profils, listes de classement, catalogue d'outils.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::model::{MemPriority, PowerPlan, Priority, PurgeScope};

/// Configuration embarquée dans le binaire.
pub const DEFAULT_TOML: &str = include_str!("../../../config/default.toml");

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub default_profile: String,
    pub poll_seconds: u64,
    pub release_after_polls: u32,
    pub profiles: BTreeMap<String, Profile>,
    pub lists: Lists,
    #[serde(default)]
    pub packs: BTreeMap<String, Pack>,
    #[serde(default)]
    pub tools: Vec<Tool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub label: String,
    pub description: String,
    pub game_mode: bool,
    pub background_priority: Priority,
    pub background_ecoqos: bool,
    pub background_memory_priority: MemPriority,
    pub trim_background: bool,
    pub purge_standby: PurgeScope,
    pub purge_when_free_below_percent: u64,
    pub power_plan: PowerPlan,
    pub shutdown_wsl: bool,
    /// Relègue l'arrière-plan sur les cœurs économes / la puce sans cache 3D.
    #[serde(default)]
    pub background_cpu_sets: bool,
    /// Services arrêtés pendant la partie et relancés ensuite (jamais un protégé).
    #[serde(default)]
    pub pause_services: Vec<String>,
    /// Purge du cache de priorité 0 en pleine partie quand la RAM se tend.
    #[serde(default)]
    pub watch_ram: bool,
    /// Mode Quotidien : allègement permanent des applis inactives, hors jeu aussi.
    #[serde(default)]
    pub daily: bool,
    /// Inactive depuis ce délai : EcoQoS, priorité mémoire basse, cœurs économes.
    #[serde(default = "default_eco_minutes")]
    pub daily_eco_after_minutes: u64,
    /// Inactive depuis ce délai : sa mémoire de travail est rendue.
    #[serde(default = "default_trim_minutes")]
    pub daily_trim_after_minutes: u64,
    /// Purge du cache de priorité 0 quand la RAM libre passe sous ce seuil (0 = jamais).
    #[serde(default = "default_daily_purge")]
    pub daily_purge_below_percent: u64,
    #[serde(default)]
    pub tool_packs: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lists {
    pub protected: Vec<String>,
    pub companions: Vec<String>,
    #[serde(default)]
    pub games: Vec<String>,
    pub game_roots: Vec<String>,
    #[serde(default)]
    pub game_helpers: Vec<String>,
    /// Applications gelées pendant la partie (vide par défaut : c'est à l'utilisateur
    /// de les désigner, un programme gelé ne répond plus).
    #[serde(default)]
    pub suspend_in_game: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pack {
    pub label: String,
    pub tools: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ToolSource {
    Winget,
    WslDistro,
    KaliApt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GamingConflict {
    None,
    Warn,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tool {
    pub id: String,
    pub name: String,
    pub source: ToolSource,
    pub package: String,
    pub gaming_conflict: GamingConflict,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub processes: Vec<String>,
    #[serde(default)]
    pub requires: Vec<String>,
}

fn default_eco_minutes() -> u64 {
    5
}
fn default_trim_minutes() -> u64 {
    30
}
fn default_daily_purge() -> u64 {
    15
}

impl Config {
    pub fn parse(text: &str) -> Result<Config, String> {
        let cfg: Config = toml::from_str(text).map_err(|e| format!("configuration illisible : {e}"))?;
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn builtin() -> Config {
        Config::parse(DEFAULT_TOML).expect("la configuration embarquée est validée par les tests")
    }

    pub fn profile(&self, name: &str) -> Result<&Profile, String> {
        self.profiles.get(name).ok_or_else(|| {
            let known: Vec<&str> = self.profiles.keys().map(String::as_str).collect();
            format!("profil inconnu « {name} » (connus : {})", known.join(", "))
        })
    }

    pub fn tool(&self, id: &str) -> Option<&Tool> {
        self.tools.iter().find(|t| t.id == id)
    }

    /// Refuse une configuration incohérente plutôt que de se comporter au hasard.
    pub fn validate(&self) -> Result<(), String> {
        let mut errors = Vec::new();
        if !self.profiles.contains_key(&self.default_profile) {
            errors.push(format!("default_profile « {} » n'existe pas", self.default_profile));
        }
        if self.poll_seconds == 0 || self.poll_seconds > 60 {
            errors.push("poll_seconds doit être entre 1 et 60".into());
        }
        if self.release_after_polls == 0 {
            errors.push("release_after_polls doit valoir au moins 1".into());
        }
        for (name, p) in &self.profiles {
            if p.daily && p.daily_trim_after_minutes < p.daily_eco_after_minutes {
                errors.push(format!(
                    "profil {name} : daily_trim_after_minutes doit suivre daily_eco_after_minutes"
                ));
            }
            if p.daily && p.daily_eco_after_minutes == 0 {
                errors.push(format!(
                    "profil {name} : daily_eco_after_minutes doit valoir au moins 1"
                ));
            }
            if p.purge_when_free_below_percent > 100 {
                errors.push(format!("profil {name} : purge_when_free_below_percent > 100"));
            }
            let catalog = crate::allege::Catalog::builtin();
            for svc in &p.pause_services {
                if let Some(reason) = catalog.protection(svc) {
                    errors.push(format!("profil {name} : le service {svc} est protégé ({reason})"));
                }
            }
            for pack in &p.tool_packs {
                if !self.packs.contains_key(pack) {
                    errors.push(format!("profil {name} : pack inconnu « {pack} »"));
                }
            }
        }
        let lists = [
            ("protected", &self.lists.protected),
            ("companions", &self.lists.companions),
            ("games", &self.lists.games),
            ("game_roots", &self.lists.game_roots),
            ("game_helpers", &self.lists.game_helpers),
            ("suspend_in_game", &self.lists.suspend_in_game),
        ];
        for (list, items) in lists {
            for item in items {
                if item.chars().any(|c| c.is_uppercase()) {
                    errors.push(format!("{list} : « {item} » doit être en minuscules"));
                }
            }
        }
        let mut ids = HashSet::new();
        for t in &self.tools {
            if !ids.insert(t.id.as_str()) {
                errors.push(format!("outil en double : {}", t.id));
            }
            if let Err(e) = crate::tools::validate_package(t) {
                errors.push(e);
            }
            if t.gaming_conflict == GamingConflict::Warn && t.reason.is_none() {
                errors.push(format!("outil {} : un conflit doit dire pourquoi (reason)", t.id));
            }
            for r in &t.requires {
                if self.tool(r).is_none() {
                    errors.push(format!("outil {} : requiert « {r} » absent du catalogue", t.id));
                }
            }
        }
        for (name, pack) in &self.packs {
            for t in &pack.tools {
                if self.tool(t).is_none() {
                    errors.push(format!("pack {name} : outil inconnu « {t} »"));
                }
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("\n"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_config_is_valid() {
        let cfg = Config::builtin();
        assert_eq!(cfg.default_profile, "gaming");
        for p in ["gaming", "balanced", "cyber"] {
            cfg.profile(p).unwrap();
        }
    }

    #[test]
    fn gaming_profile_follows_the_spec() {
        let cfg = Config::builtin();
        let g = cfg.profile("gaming").unwrap();
        assert!(g.game_mode && g.trim_background && g.shutdown_wsl);
        assert_eq!(g.purge_standby, PurgeScope::Low, "jamais tout le cache par défaut");
        assert_eq!(g.background_memory_priority, MemPriority::Low);
        assert!(!cfg.profile("cyber").unwrap().game_mode);
    }

    #[test]
    fn anticheats_are_protected_by_default() {
        let cfg = Config::builtin();
        for name in [
            "vgc.exe",
            "easyanticheat_eos.exe",
            "beservice_x64.exe",
            "faceitservice.exe",
        ] {
            assert!(
                crate::glob::any_matches(&cfg.lists.protected, name),
                "{name} doit être protégé"
            );
        }
    }

    #[test]
    fn rejects_inconsistent_config() {
        let bad = DEFAULT_TOML.replace("default_profile = \"gaming\"", "default_profile = \"absent\"");
        assert!(Config::parse(&bad).unwrap_err().contains("absent"));

        let bad = DEFAULT_TOML.replace("\"vgc.exe\"", "\"VGC.exe\"");
        assert!(Config::parse(&bad).unwrap_err().contains("minuscules"));

        let bad = DEFAULT_TOML.replace("tools = [\"wireshark\",", "tools = [\"inexistant\",");
        assert!(Config::parse(&bad).unwrap_err().contains("inexistant"));

        let bad = DEFAULT_TOML.replace("poll_seconds = 2", "poll_seconds = 2\nfaute_de_frappe = 1");
        assert!(Config::parse(&bad).is_err(), "une clé inconnue doit être refusée");
    }

    #[test]
    fn utf8_bom_from_windows_editors_is_tolerated() {
        let with_bom = format!("\u{feff}{DEFAULT_TOML}");
        assert!(Config::parse(with_bom.trim_start_matches('\u{feff}')).is_ok());
    }

    #[test]
    fn unknown_profile_lists_known_ones() {
        let err = Config::builtin().profile("turbo").unwrap_err();
        assert!(err.contains("gaming") && err.contains("cyber"));
    }
}
