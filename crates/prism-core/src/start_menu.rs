//! Menu Démarrer de Prism (bouton Démarrer de la Prism Bar, Alt+F1) : applis, recherche,
//! épinglées, alimentation. Logique pure ; la fenêtre est dessinée par `prism-win`.
//!
//! Les applis viennent de la liste de démarrage de Windows (`Get-StartApps`) : applis
//! de bureau et du Store en une seule source, chacune avec l'identifiant que
//! `explorer.exe shell:AppsFolder\<id>` sait lancer — comme le menu de Windows.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct App {
    pub name: String,
    pub id: String,
}

/// Sortie de `Get-StartApps | ConvertTo-Json` : un objet seul ou un tableau
/// `{"Name": …, "AppID": …}`. Doublons et entrées inutiles (désinstalleurs, fichiers
/// d'aide) retirés, tri alphabétique.
pub fn parse_start_apps(json: &str) -> Vec<App> {
    #[derive(Deserialize)]
    struct Raw {
        #[serde(rename = "Name")]
        name: Option<String>,
        #[serde(rename = "AppID")]
        id: Option<String>,
    }
    let raws: Vec<Raw> = serde_json::from_str::<Vec<Raw>>(json)
        .or_else(|_| serde_json::from_str::<Raw>(json).map(|r| vec![r]))
        .unwrap_or_default();
    let mut out: Vec<App> = raws
        .into_iter()
        .filter_map(|r| Some((r.name?.trim().to_string(), r.id?.trim().to_string())))
        .filter(|(n, id)| !n.is_empty() && !id.is_empty() && !junk(n))
        .map(|(name, id)| App { name, id })
        .collect();
    out.sort_by_key(|a| fold(&a.name));
    out.dedup_by(|a, b| a.id.eq_ignore_ascii_case(&b.id));
    out
}

/// Cache disque de la liste (lecture instantanée au démarrage de la barre).
pub fn to_cache(apps: &[App]) -> Vec<u8> {
    serde_json::to_vec(apps).unwrap_or_default()
}

pub fn from_cache(bytes: &[u8]) -> Option<Vec<App>> {
    serde_json::from_slice(bytes).ok()
}

fn junk(name: &str) -> bool {
    let f = fold(name);
    ["uninstall", "desinstaller", "readme", "lisezmoi", "release notes"]
        .iter()
        .any(|w| f.contains(w))
}

/// Minuscules sans accents : « édge » trouve « Microsoft Edge ».
pub fn fold(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'à' | 'â' | 'ä' | 'á' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' | 'í' => 'i',
            'ô' | 'ö' | 'ó' => 'o',
            'ù' | 'û' | 'ü' | 'ú' => 'u',
            'ç' => 'c',
            other => other,
        })
        .collect()
}

/// Pertinence (plus petit = mieux) ; `None` : ne correspond pas.
fn score(name: &str, q: &str) -> Option<u8> {
    let n = fold(name);
    if n.starts_with(q) {
        return Some(0);
    }
    let words: Vec<&str> = n
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    if words.iter().any(|w| w.starts_with(q)) {
        return Some(1);
    }
    if n.contains(q) {
        return Some(2);
    }
    // Initiales : « vsc » pour Visual Studio Code.
    let initials: String = words.iter().filter_map(|w| w.chars().next()).collect();
    if q.len() >= 2 && initials.starts_with(q) {
        return Some(3);
    }
    None
}

/// Indices des applis à montrer : sans recherche, les épinglées (dans leur ordre) puis
/// toutes les autres ; avec une recherche, les correspondances par pertinence.
pub fn results(apps: &[App], query: &str, pinned: &[String]) -> Vec<usize> {
    let q = fold(query.trim());
    if q.is_empty() {
        let mut out: Vec<usize> = pinned
            .iter()
            .filter_map(|id| apps.iter().position(|a| a.id.eq_ignore_ascii_case(id)))
            .collect();
        let rest: Vec<usize> = (0..apps.len()).filter(|i| !out.contains(i)).collect();
        out.extend(rest);
        return out;
    }
    let mut hits: Vec<(u8, bool, usize)> = apps
        .iter()
        .enumerate()
        .filter_map(|(i, a)| score(&a.name, &q).map(|s| (s, !pinned.iter().any(|p| p.eq_ignore_ascii_case(&a.id)), i)))
        .collect();
    hits.sort();
    hits.into_iter().map(|(_, _, i)| i).collect()
}

/// Touches que le menu comprend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Backspace,
    Up,
    Down,
    Enter,
    Escape,
}

/// Ce que le menu demande à la fenêtre.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Rien,
    Redessiner,
    Lancer(String),
    Fermer,
}

/// État du menu ouvert.
#[derive(Clone, Debug, Default)]
pub struct Menu {
    pub query: String,
    /// Rang sélectionné dans `shown`.
    pub sel: usize,
    /// Premier rang affiché (défilement).
    pub scroll: usize,
    pub shown: Vec<usize>,
}

impl Menu {
    pub fn open(apps: &[App], pinned: &[String]) -> Menu {
        Menu {
            shown: results(apps, "", pinned),
            ..Default::default()
        }
    }

    pub fn refresh(&mut self, apps: &[App], pinned: &[String]) {
        self.shown = results(apps, &self.query, pinned);
        self.sel = self.sel.min(self.shown.len().saturating_sub(1));
    }

    /// Une touche ; `visible` : nombre de lignes affichables (pour le défilement).
    pub fn key(&mut self, k: Key, apps: &[App], pinned: &[String], visible: usize) -> Action {
        match k {
            Key::Char(c) if !c.is_control() => {
                self.query.push(c);
                self.sel = 0;
                self.scroll = 0;
                self.refresh(apps, pinned);
            }
            Key::Char(_) => return Action::Rien,
            Key::Backspace => {
                if self.query.pop().is_none() {
                    return Action::Rien;
                }
                self.sel = 0;
                self.scroll = 0;
                self.refresh(apps, pinned);
            }
            Key::Up => self.sel = self.sel.saturating_sub(1),
            Key::Down => self.sel = (self.sel + 1).min(self.shown.len().saturating_sub(1)),
            Key::Enter => {
                return match self.shown.get(self.sel) {
                    Some(&i) => Action::Lancer(apps[i].id.clone()),
                    None => Action::Rien,
                }
            }
            Key::Escape => {
                if self.query.is_empty() {
                    return Action::Fermer;
                }
                self.query.clear();
                self.sel = 0;
                self.scroll = 0;
                self.refresh(apps, pinned);
            }
        }
        // La sélection reste visible.
        let visible = visible.max(1);
        if self.sel < self.scroll {
            self.scroll = self.sel;
        } else if self.sel >= self.scroll + visible {
            self.scroll = self.sel + 1 - visible;
        }
        Action::Redessiner
    }

    pub fn wheel(&mut self, lines: i32, visible: usize) {
        let max = self.shown.len().saturating_sub(visible.max(1));
        self.scroll = (self.scroll as i32 - lines).clamp(0, max as i32) as usize;
    }
}

/// Épingle ou retire une appli (identifiants dans `bar.json`, `start_pinned`).
pub fn toggle_pin(pinned: &mut Vec<String>, id: &str) -> bool {
    if let Some(i) = pinned.iter().position(|p| p.eq_ignore_ascii_case(id)) {
        pinned.remove(i);
        false
    } else {
        pinned.push(id.to_string());
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Power {
    Verrouiller,
    Veille,
    Redemarrer,
    Arreter,
}

impl Power {
    pub const ALL: [Power; 4] = [Power::Verrouiller, Power::Veille, Power::Redemarrer, Power::Arreter];

    pub fn label(self) -> &'static str {
        match self {
            Power::Verrouiller => "Verrouiller",
            Power::Veille => "Veille",
            Power::Redemarrer => "Redémarrer",
            Power::Arreter => "Arrêter",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apps() -> Vec<App> {
        parse_start_apps(
            r#"[
            {"Name":"Visual Studio Code","AppID":"Microsoft.VisualStudioCode"},
            {"Name":"Microsoft Edge","AppID":"MSEdge"},
            {"Name":"Calculatrice","AppID":"Microsoft.WindowsCalculator_8wekyb3d8bbwe!App"},
            {"Name":"Paramètres","AppID":"windows.immersivecontrolpanel_cw5n1h2txyewy!microsoft.windows.immersivecontrolpanel"},
            {"Name":"Discord","AppID":"com.squirrel.Discord.Discord"},
            {"Name":"Désinstaller Discord","AppID":"{6D809377}\\Discord\\Uninstall.exe"},
            {"Name":"Microsoft Edge","AppID":"MSEdge"},
            {"Name":"Steam","AppID":"Valve.Steam.Client"}
        ]"#,
        )
    }

    fn names(a: &[App], idx: &[usize]) -> Vec<String> {
        idx.iter().map(|&i| a[i].name.clone()).collect()
    }

    #[test]
    fn start_apps_are_parsed_deduplicated_and_cleaned() {
        let a = apps();
        assert_eq!(a.len(), 6);
        assert!(a.iter().all(|x| !x.name.contains("Désinstaller")));
        assert_eq!(a[0].name, "Calculatrice");
        // Un seul objet (une seule appli) est aussi accepté.
        assert_eq!(parse_start_apps(r#"{"Name":"Steam","AppID":"x"}"#).len(), 1);
        assert!(parse_start_apps("pas du json").is_empty());
    }

    #[test]
    fn search_ranks_prefix_then_word_then_initials_and_ignores_accents() {
        let a = apps();
        assert_eq!(names(&a, &results(&a, "ed", &[])), ["Microsoft Edge"]);
        assert_eq!(names(&a, &results(&a, "vsc", &[])), ["Visual Studio Code"]);
        assert_eq!(names(&a, &results(&a, "parametres", &[])), ["Paramètres"]);
        let s = names(&a, &results(&a, "s", &[]));
        assert_eq!(s[0], "Steam"); // commence par « s »
        assert!(s.contains(&"Visual Studio Code".to_string())); // mot « studio »
    }

    #[test]
    fn pinned_apps_come_first_in_their_order() {
        let a = apps();
        let pinned = vec![
            "Valve.Steam.Client".to_string(),
            "com.squirrel.Discord.Discord".to_string(),
        ];
        let r = names(&a, &results(&a, "", &pinned));
        assert_eq!(&r[..2], ["Steam", "Discord"]);
        assert_eq!(r.len(), 6);
    }

    #[test]
    fn keyboard_types_moves_launches_and_closes() {
        let a = apps();
        let mut m = Menu::open(&a, &[]);
        for c in "dis".chars() {
            m.key(Key::Char(c), &a, &[], 8);
        }
        assert_eq!(
            m.key(Key::Enter, &a, &[], 8),
            Action::Lancer("com.squirrel.Discord.Discord".into())
        );
        // Échap efface d'abord la recherche, puis ferme.
        assert_eq!(m.key(Key::Escape, &a, &[], 8), Action::Redessiner);
        assert!(m.query.is_empty() && m.shown.len() == 6);
        assert_eq!(m.key(Key::Escape, &a, &[], 8), Action::Fermer);
    }

    #[test]
    fn selection_scrolls_into_view() {
        let a = apps();
        let mut m = Menu::open(&a, &[]);
        for _ in 0..4 {
            m.key(Key::Down, &a, &[], 2);
        }
        assert_eq!(m.sel, 4);
        assert_eq!(m.scroll, 3);
        m.wheel(10, 2);
        assert_eq!(m.scroll, 0);
    }

    #[test]
    fn pin_toggles() {
        let mut p = Vec::new();
        assert!(toggle_pin(&mut p, "A"));
        assert!(!toggle_pin(&mut p, "a"));
        assert!(p.is_empty());
    }
}
