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
            if p.purge_when_free_below_percent > 100 {
                errors.push(format!("profil {name} : purge_when_free_below_percent > 100"));
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
    fn unknown_profile_lists_known_ones() {
        let err = Config::builtin().profile("turbo").unwrap_err();
        assert!(err.contains("gaming") && err.contains("cyber"));
    }
}
