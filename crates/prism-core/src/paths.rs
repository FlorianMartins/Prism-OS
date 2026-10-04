//! Emplacements et état persistant, partagés par `prism.exe` et l'interface.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::Config;

/// `%ProgramData%\Prism` sous Windows, `~/.prism` ailleurs (développement).
pub fn data_dir() -> PathBuf {
    if cfg!(windows) {
        let base = std::env::var_os("ProgramData").unwrap_or_else(|| "C:\\ProgramData".into());
        PathBuf::from(base).join("Prism")
    } else {
        let home = std::env::var_os("HOME").unwrap_or_else(|| ".".into());
        PathBuf::from(home).join(".prism")
    }
}

pub fn config_path() -> PathBuf {
    data_dir().join("config.toml")
}

pub fn journal_path() -> PathBuf {
    data_dir().join("journal.json")
}

/// Configuration de l'utilisateur si elle existe, sinon celle embarquée.
pub fn load_config() -> Result<Config, String> {
    let path = config_path();
    match fs::read_to_string(&path) {
        // Un éditeur Windows (Bloc-notes, PowerShell 5) peut ajouter une marque UTF-8.
        Ok(text) => {
            Config::parse(text.trim_start_matches('\u{feff}')).map_err(|e| format!("{} :\n{e}", path.display()))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::builtin()),
        Err(e) => Err(format!("{} : {e}", path.display())),
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct State {
    pub profile: Option<String>,
}

pub fn load_state() -> State {
    fs::read(data_dir().join("state.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save_state(state: &State) -> Result<(), String> {
    let dir = data_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("{} : {e}", dir.display()))?;
    let json = serde_json::to_vec_pretty(state).map_err(|e| e.to_string())?;
    fs::write(dir.join("state.json"), json).map_err(|e| e.to_string())
}

/// Profil actif : celui choisi par l'utilisateur, sinon celui par défaut. Un profil
/// mémorisé qui n'existe plus dans la configuration retombe sur le défaut.
pub fn active_profile(cfg: &Config) -> String {
    load_state()
        .profile
        .filter(|p| cfg.profiles.contains_key(p))
        .unwrap_or_else(|| cfg.default_profile.clone())
}

pub fn etat_path() -> PathBuf {
    data_dir().join("etat.json")
}
