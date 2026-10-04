//! Journal des changements réversibles. Écrit après chaque changement pour qu'un crash
//! en pleine partie n'en perde aucun (spec v0.1 §7).

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::model::{EcoState, MemPriority, Priority, Target};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "undo", rename_all = "snake_case")]
pub enum Undo {
    Priority {
        target: Target,
        previous: Priority,
    },
    EcoQos {
        target: Target,
        previous: EcoState,
    },
    MemoryPriority {
        target: Target,
        previous: MemPriority,
    },
    /// GUID du plan d'alimentation d'origine.
    PowerPlan {
        previous: String,
    },
    /// CPU sets d'origine ; vide = aucun (Windows choisit librement).
    CpuSets {
        target: Target,
        previous: Vec<u32>,
    },
    /// Service mis en pause, à relancer.
    Service {
        name: String,
    },
    /// Processus gelé, à dégeler.
    Resume {
        target: Target,
    },
}

impl Undo {
    pub fn describe(&self) -> String {
        match self {
            Undo::Priority { target, previous } => format!("{} : priorité CPU <- {previous:?}", target.name),
            Undo::EcoQos { target, previous } => format!("{} : EcoQoS <- {previous:?}", target.name),
            Undo::MemoryPriority { target, previous } => {
                format!("{} : priorité mémoire <- {previous:?}", target.name)
            }
            Undo::PowerPlan { previous } => format!("plan d'alimentation <- {previous}"),
            Undo::CpuSets { target, previous } if previous.is_empty() => {
                format!("{} : tous les cœurs rendus", target.name)
            }
            Undo::CpuSets { target, previous } => format!("{} : {} cœurs d'origine", target.name, previous.len()),
            Undo::Service { name } => format!("service {name} relancé"),
            Undo::Resume { target } => format!("{} : dégelé", target.name),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Journal {
    pub profile: String,
    pub entries: Vec<Undo>,
}

pub trait JournalStore {
    fn save(&mut self, journal: &Journal) -> Result<(), String>;
    fn load(&mut self) -> Result<Option<Journal>, String>;
    fn clear(&mut self) -> Result<(), String>;
}

/// Journal sur disque, écrit de façon atomique (fichier temporaire puis renommage) :
/// un crash de Prism pendant l'écriture laisse l'ancienne version intacte.
///
/// Pas de `fsync` : le journal protège contre la mort de *Prism*, pour laquelle le cache
/// du système suffit. Une coupure de courant, elle, tue aussi les processus dont on a
/// changé la priorité, et le plan d'alimentation est restauré au prochain démarrage de
/// Prism dans tous les cas où le fichier a atteint le disque. Forcer l'écriture coûterait
/// des centaines d'accès disque synchrones au lancement du jeu, le pire moment.
pub struct FileStore {
    pub path: PathBuf,
}

impl JournalStore for FileStore {
    fn save(&mut self, journal: &Journal) -> Result<(), String> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let tmp = self.path.with_extension("tmp");
        let json = serde_json::to_vec_pretty(journal).map_err(|e| e.to_string())?;
        let mut f = fs::File::create(&tmp).map_err(|e| format!("{}: {e}", tmp.display()))?;
        f.write_all(&json).map_err(|e| e.to_string())?;
        drop(f);
        fs::rename(&tmp, &self.path).map_err(|e| format!("{}: {e}", self.path.display()))
    }

    fn load(&mut self) -> Result<Option<Journal>, String> {
        match fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|e| format!("journal illisible {} : {e}", self.path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("{}: {e}", self.path.display())),
        }
    }

    fn clear(&mut self) -> Result<(), String> {
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("{}: {e}", self.path.display())),
        }
    }
}

/// Journal en mémoire (tests, et nettoyage manuel qui restaure dans la foulée).
#[derive(Default)]
pub struct MemStore {
    pub saved: Option<Journal>,
    pub saves: usize,
}

impl JournalStore for MemStore {
    fn save(&mut self, journal: &Journal) -> Result<(), String> {
        self.saved = Some(journal.clone());
        self.saves += 1;
        Ok(())
    }
    fn load(&mut self) -> Result<Option<Journal>, String> {
        Ok(self.saved.clone())
    }
    fn clear(&mut self) -> Result<(), String> {
        self.saved = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ProcId;

    #[test]
    fn file_store_round_trip_and_clear() {
        let dir = std::env::temp_dir().join(format!("prism-journal-{}", std::process::id()));
        let mut store = FileStore {
            path: dir.join("journal.json"),
        };
        assert_eq!(store.load().unwrap(), None);
        let j = Journal {
            profile: "gaming".into(),
            entries: vec![
                Undo::Priority {
                    target: Target {
                        id: ProcId { pid: 7, created: 42 },
                        name: "a.exe".into(),
                    },
                    previous: Priority::Normal,
                },
                Undo::PowerPlan {
                    previous: "381b4222-f694-41f0-9685-ff5bb260df2e".into(),
                },
            ],
        };
        store.save(&j).unwrap();
        assert_eq!(store.load().unwrap(), Some(j));
        assert!(!dir.join("journal.tmp").exists(), "pas de fichier temporaire laissé");
        store.clear().unwrap();
        store.clear().unwrap();
        assert_eq!(store.load().unwrap(), None);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn corrupt_journal_is_an_error_not_silence() {
        let dir = std::env::temp_dir().join(format!("prism-journal-bad-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("journal.json");
        fs::write(&path, b"{ pas du json").unwrap();
        assert!(FileStore { path }.load().is_err());
        let _ = fs::remove_dir_all(dir);
    }
}
