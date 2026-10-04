//! Lecture de la bibliothèque de jeux installés (fichiers des lanceurs et registre).

use std::path::{Path, PathBuf};
use std::ptr::null_mut;

use prism_core::library::{epic_game, steam_app_manifest, steam_game, steam_library_folders, Game, Launch, Store};
use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};

use crate::startup::subkeys;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Valeur texte du registre, ou `None`.
fn reg_sz(root: HKEY, sub: &str, value: &str) -> Option<String> {
    let k = wide(sub);
    let v = wide(value);
    let mut buf = [0u16; 1024];
    let mut len = (buf.len() * 2) as u32;
    // SAFETY: tampon local de capacité annoncée en octets.
    let st = unsafe {
        RegGetValueW(
            root,
            k.as_ptr(),
            v.as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            buf.as_mut_ptr() as *mut _,
            &mut len,
        )
    };
    if st != 0 {
        return None;
    }
    let n = (len as usize / 2).saturating_sub(1).min(buf.len());
    Some(String::from_utf16_lossy(&buf[..n]))
}

fn steam() -> Vec<Game> {
    let root = reg_sz(HKEY_CURRENT_USER, r"Software\Valve\Steam", "SteamPath")
        .or_else(|| reg_sz(HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Valve\Steam", "InstallPath"));
    let Some(root) = root else { return Vec::new() };
    let root = root.replace('/', "\\");
    let vdf = std::fs::read_to_string(Path::new(&root).join(r"steamapps\libraryfolders.vdf")).unwrap_or_default();
    let mut libraries = steam_library_folders(&vdf);
    if libraries.is_empty() {
        libraries.push(root);
    }
    let mut games = Vec::new();
    for lib in libraries {
        let Ok(rd) = std::fs::read_dir(Path::new(&lib).join("steamapps")) else {
            continue;
        };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_lowercase();
            if !(name.starts_with("appmanifest_") && name.ends_with(".acf")) {
                continue;
            }
            let Ok(acf) = std::fs::read_to_string(e.path()) else {
                continue;
            };
            if let Some((id, title, dir)) = steam_app_manifest(&acf) {
                games.push(steam_game(&lib, &id, &title, &dir));
            }
        }
    }
    games
}

fn epic() -> Vec<Game> {
    let Some(pd) = std::env::var_os("ProgramData") else {
        return Vec::new();
    };
    let dir = PathBuf::from(pd).join(r"Epic\EpicGamesLauncher\Data\Manifests");
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    rd.flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("item")))
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .filter_map(|json| epic_game(&json))
        .collect()
}

fn gog() -> Vec<Game> {
    const KEY: &str = r"SOFTWARE\WOW6432Node\GOG.com\Games";
    subkeys(HKEY_LOCAL_MACHINE, KEY)
        .into_iter()
        .filter_map(|id| {
            let sub = format!(r"{KEY}\{id}");
            let name = reg_sz(HKEY_LOCAL_MACHINE, &sub, "gameName")?;
            let path = reg_sz(HKEY_LOCAL_MACHINE, &sub, "path")?;
            let launch = reg_sz(HKEY_LOCAL_MACHINE, &sub, "exe")
                .map(Launch::Exe)
                .unwrap_or(Launch::None);
            Some(Game {
                name,
                store: Store::Gog,
                install_dir: path,
                launch,
            })
        })
        .collect()
}

fn battle_net() -> Vec<Game> {
    const KEY: &str = r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall";
    subkeys(HKEY_LOCAL_MACHINE, KEY)
        .into_iter()
        .filter_map(|id| {
            let sub = format!(r"{KEY}\{id}");
            let publisher = reg_sz(HKEY_LOCAL_MACHINE, &sub, "Publisher")?;
            let name = reg_sz(HKEY_LOCAL_MACHINE, &sub, "DisplayName")?;
            if publisher != "Blizzard Entertainment" || name.eq_ignore_ascii_case("Battle.net") {
                return None;
            }
            let dir = reg_sz(HKEY_LOCAL_MACHINE, &sub, "InstallLocation")?;
            Some(Game {
                name,
                store: Store::BattleNet,
                install_dir: dir,
                launch: Launch::None,
            })
        })
        .collect()
}

/// Tous les jeux installés détectés, triés par nom.
pub fn installed_games() -> Vec<Game> {
    let mut all: Vec<Game> = [steam(), epic(), gog(), battle_net()].into_iter().flatten().collect();
    all.sort_by_key(|g| g.name.to_lowercase());
    all.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name) && a.store == b.store);
    all
}

/// Exécutables du dossier d'un jeu (chemin relatif, taille), sur 4 niveaux au plus et
/// 20 000 entrées au plus (un dossier de jeu peut contenir des centaines de milliers de
/// fichiers de données).
pub fn game_files(dir: &str) -> Vec<(String, u64)> {
    let root = std::path::Path::new(dir);
    let mut out = Vec::new();
    let mut stack = vec![(root.to_path_buf(), 0u32)];
    let mut seen = 0usize;
    while let Some((d, depth)) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            seen += 1;
            if seen > 20_000 {
                return out;
            }
            let Ok(ft) = e.file_type() else { continue };
            let p = e.path();
            if ft.is_dir() && depth < 4 {
                stack.push((p, depth + 1));
            } else if ft.is_file() && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("exe")) {
                if let (Ok(rel), Ok(meta)) = (p.strip_prefix(root), e.metadata()) {
                    out.push((rel.display().to_string(), meta.len()));
                }
            }
        }
    }
    out
}
