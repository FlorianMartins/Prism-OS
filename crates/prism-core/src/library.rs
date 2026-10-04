//! Bibliothèque de jeux installés (Steam, Epic, GOG, Battle.net) : alimente le lanceur
//! et la détection du Mode Jeu (un jeu installé hors des dossiers connus devient
//! reconnaissable par son dossier d'installation).
//!
//! Ici : les parseurs, purs et testés. La lecture des fichiers et du registre est
//! côté Windows (`prism-win/src/library.rs`).

use serde::Deserialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Store {
    Steam,
    Epic,
    Gog,
    BattleNet,
}

impl Store {
    pub fn label(self) -> &'static str {
        match self {
            Store::Steam => "Steam",
            Store::Epic => "Epic",
            Store::Gog => "GOG",
            Store::BattleNet => "Battle.net",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Launch {
    /// Lien que le lanceur du magasin sait ouvrir (`steam://…`, `com.epicgames.launcher://…`).
    Uri(String),
    /// Exécutable à lancer directement.
    Exe(String),
    /// Pas de lancement direct connu (le jeu reste détecté pour le Mode Jeu).
    None,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    pub name: String,
    pub store: Store,
    pub install_dir: String,
    pub launch: Launch,
}

// --- VDF (Steam) -------------------------------------------------------------

/// Jetons d'un fichier VDF/ACF : chaînes entre guillemets et accolades.
fn vdf_tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' | '}' => out.push(c.to_string()),
            '"' => {
                let mut s = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => {
                            if let Some(n) = chars.next() {
                                s.push(n);
                            }
                        }
                        '"' => break,
                        _ => s.push(c),
                    }
                }
                out.push(s);
            }
            _ => {}
        }
    }
    out
}

/// Valeurs d'une clé donnée, à n'importe quelle profondeur (`"path" "D:\\SteamLibrary"`).
fn vdf_values(text: &str, key: &str) -> Vec<String> {
    let t = vdf_tokens(text);
    t.windows(2)
        .filter(|w| w[0].eq_ignore_ascii_case(key) && w[1] != "{" && w[1] != "}")
        .map(|w| w[1].clone())
        .collect()
}

/// Dossiers de bibliothèque Steam déclarés dans `libraryfolders.vdf`.
pub fn steam_library_folders(vdf: &str) -> Vec<String> {
    vdf_values(vdf, "path")
}

/// Applications Steam qui ne sont pas des jeux.
const STEAM_NOT_GAMES: [&str; 4] = ["228980", "1070560", "1391110", "1628350"];

/// (appid, nom, dossier d'installation relatif à `steamapps\common`) d'un `appmanifest_*.acf`.
pub fn steam_app_manifest(acf: &str) -> Option<(String, String, String)> {
    let first = |k: &str| vdf_values(acf, k).into_iter().next();
    let appid = first("appid")?;
    if STEAM_NOT_GAMES.contains(&appid.as_str()) {
        return None;
    }
    Some((appid, first("name")?, first("installdir")?))
}

pub fn steam_game(library: &str, appid: &str, name: &str, installdir: &str) -> Game {
    Game {
        name: name.to_string(),
        store: Store::Steam,
        install_dir: format!(r"{}\steamapps\common\{installdir}", library.trim_end_matches('\\')),
        launch: Launch::Uri(format!("steam://rungameid/{appid}")),
    }
}

// --- Epic --------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct EpicItem {
    display_name: String,
    install_location: String,
    app_name: String,
    catalog_namespace: String,
    catalog_item_id: String,
    // Epic écrit « bIsIncompleteInstall » : la règle PascalCase donnerait « BIs… ».
    #[serde(default, rename = "bIsIncompleteInstall")]
    b_is_incomplete_install: bool,
    #[serde(default)]
    app_categories: Vec<String>,
}

/// Un manifeste `.item` du lanceur Epic.
pub fn epic_game(item_json: &str) -> Option<Game> {
    let it: EpicItem = serde_json::from_str(item_json).ok()?;
    if it.b_is_incomplete_install {
        return None;
    }
    if !it.app_categories.is_empty() && !it.app_categories.iter().any(|c| c == "games") {
        return None;
    }
    let uri = format!(
        "com.epicgames.launcher://apps/{}%3A{}%3A{}?action=launch&silent=true",
        it.catalog_namespace, it.catalog_item_id, it.app_name
    );
    Some(Game {
        name: it.display_name,
        store: Store::Epic,
        install_dir: it.install_location,
        launch: Launch::Uri(uri),
    })
}

// --- Racines de jeu ----------------------------------------------------------

/// Dossier d'installation -> racine de jeu pour le classement (minuscules, `\` final).
/// Refuse les chemins trop génériques : prendre `C:\Program Files\` pour un jeu ferait
/// classer toutes les applis comme des jeux.
pub fn game_root(install_dir: &str) -> Option<String> {
    let p = install_dir
        .trim()
        .trim_end_matches(['\\', '/'])
        .replace('/', "\\")
        .to_lowercase();
    let parts: Vec<&str> = p.split('\\').filter(|s| !s.is_empty()).collect();
    if parts.len() < 2 {
        return None;
    }
    const GENERIC: [&str; 6] = [
        "program files",
        "program files (x86)",
        "windows",
        "users",
        "programdata",
        "games",
    ];
    if parts.len() == 2 && GENERIC.contains(&parts[1]) {
        return None;
    }
    if parts.contains(&"windows") {
        return None;
    }
    Some(format!("{p}\\"))
}

/// Toutes les racines utilisables d'une bibliothèque, sans doublons.
pub fn game_roots(games: &[Game]) -> Vec<String> {
    let mut roots: Vec<String> = games.iter().filter_map(|g| game_root(&g.install_dir)).collect();
    roots.sort();
    roots.dedup();
    roots
}

/// Recherche par nom (insensible à la casse, d'abord exact puis partiel).
pub fn find<'a>(games: &'a [Game], query: &str) -> Option<&'a Game> {
    let q = query.to_lowercase();
    games
        .iter()
        .find(|g| g.name.to_lowercase() == q)
        .or_else(|| games.iter().find(|g| g.name.to_lowercase().contains(&q)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIBRARYFOLDERS: &str = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"label"		""
		"apps"
		{
			"228980"		"0"
			"1245620"		"60000000000"
		}
	}
	"1"
	{
		"path"		"D:\\SteamLibrary"
		"apps" { "730" "3500000000" }
	}
}"#;

    const ACF: &str = r#"
"AppState"
{
	"appid"		"1245620"
	"Universe"		"1"
	"name"		"ELDEN RING"
	"installdir"		"ELDEN RING"
	"InstalledDepots" { "1245621" { "manifest" "123" } }
}"#;

    #[test]
    fn steam_library_and_manifest_are_parsed() {
        assert_eq!(
            steam_library_folders(LIBRARYFOLDERS),
            vec![r"C:\Program Files (x86)\Steam", r"D:\SteamLibrary"]
        );
        let (id, name, dir) = steam_app_manifest(ACF).unwrap();
        assert_eq!(
            (id.as_str(), name.as_str(), dir.as_str()),
            ("1245620", "ELDEN RING", "ELDEN RING")
        );
        let g = steam_game(r"D:\SteamLibrary", &id, &name, &dir);
        assert_eq!(g.install_dir, r"D:\SteamLibrary\steamapps\common\ELDEN RING");
        assert_eq!(g.launch, Launch::Uri("steam://rungameid/1245620".into()));
    }

    #[test]
    fn steamworks_redistributables_are_not_games() {
        let acf = ACF.replace("1245620", "228980");
        assert!(steam_app_manifest(&acf).is_none());
    }

    #[test]
    fn epic_manifest_is_parsed_and_non_games_skipped() {
        let item = r#"{"FormatVersion":0,"DisplayName":"Fortnite","InstallLocation":"C:\\Program Files\\Epic Games\\Fortnite",
            "AppName":"Fortnite","CatalogNamespace":"fn","CatalogItemId":"4fe75bbc5a674f4f9b356b5c90567da5",
            "bIsIncompleteInstall":false,"AppCategories":["public","games","applications"]}"#;
        let g = epic_game(item).unwrap();
        assert_eq!(g.name, "Fortnite");
        assert!(
            matches!(g.launch, Launch::Uri(ref u) if u.starts_with("com.epicgames.launcher://apps/fn%3A4fe75") && u.ends_with("action=launch&silent=true"))
        );
        let plugin = item.replace(r#"["public","games","applications"]"#, r#"["plugins","engines"]"#);
        assert!(epic_game(&plugin).is_none(), "moteur Unreal, pas un jeu");
        let partial = item.replace(r#""bIsIncompleteInstall":false"#, r#""bIsIncompleteInstall":true"#);
        assert!(epic_game(&partial).is_none());
        assert!(epic_game("{ pas du json").is_none());
    }

    #[test]
    fn game_roots_refuse_generic_folders() {
        assert_eq!(
            game_root(r"C:\Program Files (x86)\Overwatch").as_deref(),
            Some(r"c:\program files (x86)\overwatch\")
        );
        assert_eq!(game_root(r"D:\Jeux\Witcher3\").as_deref(), Some(r"d:\jeux\witcher3\"));
        for bad in [
            r"C:\",
            r"C:\Program Files",
            r"C:\Program Files (x86)\",
            r"D:\Games",
            r"C:\Windows\System32",
            "",
        ] {
            assert_eq!(game_root(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn a_detected_install_dir_makes_its_game_recognised() {
        use crate::classify::{classify, Class};
        use crate::config::Config;
        use crate::model::{ProcId, ProcInfo, Snapshot};
        let games = vec![Game {
            name: "Overwatch".into(),
            store: Store::BattleNet,
            install_dir: r"C:\Program Files (x86)\Overwatch".into(),
            launch: Launch::None,
        }];
        let mut cfg = Config::builtin();
        let p = ProcInfo {
            id: ProcId { pid: 50, created: 1 },
            name: "overwatch.exe".into(),
            path: Some(r"c:\program files (x86)\overwatch\_retail_\overwatch.exe".into()),
            session: 1,
            working_set: 0,
            cpu_time: 0,
        };
        let snap = Snapshot {
            procs: vec![p.clone()],
            user_session: 1,
            self_pid: 9,
            ..Default::default()
        };
        assert_eq!(classify(&p, &snap, &cfg), Class::Background, "avant : inconnu");
        cfg.lists.game_roots.extend(game_roots(&games));
        assert_eq!(
            classify(&p, &snap, &cfg),
            Class::Game,
            "après : reconnu par son dossier"
        );
    }

    #[test]
    fn find_prefers_exact_names() {
        let g = |n: &str| Game {
            name: n.into(),
            store: Store::Steam,
            install_dir: String::new(),
            launch: Launch::None,
        };
        let games = vec![g("Counter-Strike 2"), g("Counter")];
        assert_eq!(find(&games, "counter").unwrap().name, "Counter");
        assert_eq!(find(&games, "strike").unwrap().name, "Counter-Strike 2");
        assert!(find(&games, "zelda").is_none());
    }
}
