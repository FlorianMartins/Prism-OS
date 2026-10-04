//! État en direct écrit par `prism watch` et lu par l'interface.

use std::fs;

use serde::{Deserialize, Serialize};

use crate::model::MemStatus;
use crate::paths::etat_path;

/// Nombre d'événements récents gardés.
pub const RECENT_MAX: usize = 40;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Etat {
    /// Secondes depuis 1970 : l'interface sait si `prism watch` tourne encore.
    pub updated_unix: u64,
    pub profile: String,
    /// Jeux de la partie en cours (vide : pas de partie).
    pub game: Vec<String>,
    /// Applis allégées en ce moment par le Mode Quotidien.
    pub eased: Vec<String>,
    pub mem: MemStatus,
    /// Répartition des cœurs, si le processeur en permet une.
    pub cores: Option<String>,
    /// Derniers événements, du plus ancien au plus récent.
    pub recent: Vec<String>,
}

impl Etat {
    pub fn push_recent(&mut self, line: String) {
        self.recent.push(line);
        let excess = self.recent.len().saturating_sub(RECENT_MAX);
        self.recent.drain(..excess);
    }

    /// Même contenu, sans compter l'horodatage ni la mémoire (qui bougent sans cesse).
    pub fn same_content(&self, other: &Etat) -> bool {
        self.profile == other.profile
            && self.game == other.game
            && self.eased == other.eased
            && self.cores == other.cores
            && self.recent == other.recent
    }

    pub fn load() -> Option<Etat> {
        fs::read(etat_path()).ok().and_then(|b| serde_json::from_slice(&b).ok())
    }

    pub fn save(&self) -> Result<(), String> {
        let path = etat_path();
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, serde_json::to_vec(self).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        fs::rename(&tmp, &path).map_err(|e| e.to_string())
    }

    /// `prism watch` est-il vivant ? (mis à jour il y a moins de `max_age` secondes)
    pub fn alive(&self, now_unix: u64, max_age: u64) -> bool {
        now_unix.saturating_sub(self.updated_unix) <= max_age
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_is_bounded_and_keeps_the_latest() {
        let mut e = Etat::default();
        for i in 0..100 {
            e.push_recent(format!("ligne {i}"));
        }
        assert_eq!(e.recent.len(), RECENT_MAX);
        assert_eq!(e.recent.last().unwrap(), "ligne 99");
        assert_eq!(e.recent.first().unwrap(), &format!("ligne {}", 100 - RECENT_MAX));
    }

    #[test]
    fn memory_changes_alone_do_not_count_as_new_content() {
        let a = Etat {
            profile: "gaming".into(),
            ..Default::default()
        };
        let mut b = a.clone();
        b.mem.free = 12345;
        b.updated_unix = 99;
        assert!(a.same_content(&b));
        b.eased.push("winword.exe".into());
        assert!(!a.same_content(&b));
    }

    #[test]
    fn liveness() {
        let e = Etat {
            updated_unix: 1000,
            ..Default::default()
        };
        assert!(e.alive(1005, 10));
        assert!(!e.alive(1020, 10));
    }
}
