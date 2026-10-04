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

/// Réglages et état propres à l'utilisateur (barre, thème, effets, tuiles, apparence),
/// toujours inscriptibles par lui : `%LOCALAPPDATA%\Prism`. Les journaux qui touchent la
/// machine (allègement, vie privée, moteur) restent dans `data_dir`, en lecture seule pour
/// les utilisateurs — sinon un compte standard pourrait y écrire ce qu'une restauration
/// en administrateur appliquerait.
pub fn user_dir() -> PathBuf {
    if !cfg!(windows) {
        return data_dir();
    }
    let base = std::env::var_os("LOCALAPPDATA").unwrap_or_else(|| "C:\\Users\\Default\\AppData\\Local".into());
    let dir = PathBuf::from(base).join("Prism");
    static MIGRATED: std::sync::Once = std::sync::Once::new();
    MIGRATED.call_once(|| migrate_user_files(&dir));
    dir
}

/// Fichiers utilisateur autrefois rangés dans `data_dir` (v0.7.x) : copiés une fois
/// vers `user_dir` (là-bas ils étaient en lecture seule pour l'utilisateur, les réglages
/// de l'appli lancée sans droits administrateur n'étaient jamais enregistrés).
const USER_FILES: [&str; 6] = [
    "bar.json",
    "apparence.json",
    "fx-stats.json",
    "tuiles.json",
    "barre-windows.txt",
    "accueil-vu",
];

fn migrate_user_files(dir: &std::path::Path) {
    let old = data_dir();
    for f in USER_FILES {
        let (from, to) = (old.join(f), dir.join(f));
        if from.exists() && !to.exists() {
            let _ = fs::create_dir_all(dir);
            let _ = fs::copy(&from, &to);
        }
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

/// `PATH` avec `dir` ajouté à la fin ; `None` s'il y est déjà (casse et barre oblique
/// finale ignorées, comme le fait Windows).
pub fn path_with(path: &str, dir: &str) -> Option<String> {
    let norm = |s: &str| s.trim().trim_end_matches('\\').to_ascii_lowercase();
    if path.split(';').any(|p| norm(p) == norm(dir)) {
        return None;
    }
    let base = path.trim_end_matches(';');
    Some(if base.is_empty() {
        dir.to_string()
    } else {
        format!("{base};{dir}")
    })
}

/// `PATH` sans `dir` ; `None` s'il n'y était pas. Les autres entrées restent intactes,
/// dans leur ordre et leur écriture (`%SystemRoot%` compris).
pub fn path_without(path: &str, dir: &str) -> Option<String> {
    let norm = |s: &str| s.trim().trim_end_matches('\\').to_ascii_lowercase();
    let parts: Vec<&str> = path.split(';').collect();
    let kept: Vec<&str> = parts.iter().copied().filter(|p| norm(p) != norm(dir)).collect();
    (kept.len() != parts.len()).then(|| kept.join(";"))
}

#[cfg(test)]
mod path_tests {
    use super::*;

    #[test]
    fn install_dir_is_added_once_and_removed_without_touching_the_rest() {
        let p = r"%SystemRoot%\system32;%SystemRoot%;C:\Tools";
        let with = path_with(p, r"C:\Program Files\Prism\").unwrap();
        assert_eq!(
            with,
            r"%SystemRoot%\system32;%SystemRoot%;C:\Tools;C:\Program Files\Prism\"
        );
        assert_eq!(path_with(&with, r"c:\program files\prism"), None, "déjà présent");
        assert_eq!(path_without(&with, r"C:\Program Files\Prism").unwrap(), p);
        assert_eq!(path_without(p, r"C:\Program Files\Prism"), None);
        assert_eq!(path_with("", r"C:\P").unwrap(), r"C:\P");
        assert_eq!(path_with(r"C:\A;", r"C:\P").unwrap(), r"C:\A;C:\P");
    }
}
