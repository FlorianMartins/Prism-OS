//! Emplacements, état persistant, journal texte, arrêt propre (Ctrl-C).
// Une partie ne sert qu'aux commandes Windows (`watch`).
#![cfg_attr(not(windows), allow(dead_code))]

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use prism_core::config::Config;

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
        Ok(text) => Config::parse(&text).map_err(|e| format!("{} :\n{e}", path.display())),
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

/// Date UTC lisible sans dépendance (`2026-10-04 17:42:03Z`).
pub fn now_utc() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (days, rem) = (secs / 86_400, secs % 86_400);
    // Conversion jours -> date civile (algorithme de H. Hinnant).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Sortie : console, et en mode silencieux un fichier `prism.log` (borné à 1 Mo).
pub struct Out {
    quiet: bool,
}

impl Out {
    pub fn new(quiet: bool) -> Out {
        Out { quiet }
    }

    pub fn line(&self, text: &str) {
        if !self.quiet {
            println!("{text}");
            return;
        }
        let path = data_dir().join("prism.log");
        if fs::metadata(&path).map(|m| m.len() > 1 << 20).unwrap_or(false) {
            let _ = fs::rename(&path, path.with_extension("log.1"));
        }
        let _ = fs::create_dir_all(data_dir());
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
            let _ = writeln!(f, "{} {text}", now_utc());
        }
    }
}

static STOP: AtomicBool = AtomicBool::new(false);

pub fn stop_requested() -> bool {
    STOP.load(Ordering::SeqCst)
}

/// Dort `d` par tranches de 100 ms pour réagir vite à Ctrl-C ou à la fermeture.
pub fn sleep_interruptible(d: Duration) {
    let step = Duration::from_millis(100);
    let mut left = d;
    while !left.is_zero() && !stop_requested() {
        let s = left.min(step);
        std::thread::sleep(s);
        left -= s;
    }
}

#[cfg(windows)]
pub fn install_stop_handler() {
    use windows_sys::core::BOOL;
    use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;

    unsafe extern "system" fn handler(_ctrl: u32) -> BOOL {
        STOP.store(true, Ordering::SeqCst);
        // Laisse le temps à la boucle de restaurer avant que Windows ne termine le
        // processus (fermeture de la console : environ 5 s).
        std::thread::sleep(Duration::from_secs(3));
        1
    }
    // SAFETY: le gestionnaire est une fonction statique valable toute la vie du processus.
    unsafe { SetConsoleCtrlHandler(Some(handler), 1) };
}

#[cfg(not(windows))]
pub fn install_stop_handler() {}

/// Mode silencieux (tâche planifiée) : se détache de la console pour ne pas laisser
/// de fenêtre ouverte. La sortie part dans `prism.log`.
#[cfg(windows)]
pub fn detach_console() {
    // SAFETY: appel sans argument ; sans console attachée il échoue sans effet.
    unsafe { windows_sys::Win32::System::Console::FreeConsole() };
}

#[cfg(not(windows))]
pub fn detach_console() {}

#[cfg(test)]
mod tests {
    #[test]
    fn utc_date_has_the_expected_shape() {
        let s = super::now_utc();
        assert_eq!(s.len(), 20, "{s}");
        assert!(s.starts_with("20") && s.ends_with('Z'));
    }
}
