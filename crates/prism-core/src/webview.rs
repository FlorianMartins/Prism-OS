//! WebView en arrière-plan : une appli sans fenêtre visible depuis un moment (réduite
//! dans la zone de notification : Teams, Outlook, Widgets…) garde souvent un moteur
//! WebView2 complet en mémoire (`msedgewebview2.exe`, 100 à 400 Mo). Prism le ferme.
//!
//! Règles :
//! - une WebView appartient à l'appli qu'on trouve en remontant ses parents (dates de
//!   création comparées : un PID réutilisé n'est jamais pris pour un parent) ; sans
//!   appli connue, on n'y touche pas ;
//! - seulement les applis d'arrière-plan ordinaires : jamais un jeu, un anti-cheat, un
//!   compagnon de jeu, un processus protégé, Prism, ni une appli exclue ;
//! - l'appli doit être restée sans aucune fenêtre visible (réduite compte comme
//!   visible) pendant `minutes` ;
//! - une seule fois par lancement de l'appli : si elle recrée sa WebView, on la laisse
//!   (sinon on se battrait avec elle).
//!
//! Fermer la WebView ne ferme pas l'appli : elle la recrée quand on la rouvre (le
//! contenu se recharge). C'est la contrepartie, affichée dans l'interface.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::classify::{classify, Class};
use crate::config::Config;
use crate::model::{ProcId, ProcInfo, Snapshot};

pub const WEBVIEW: &str = "msedgewebview2.exe";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Reglages {
    pub actif: bool,
    /// Minutes sans fenêtre visible avant de fermer la WebView d'une appli.
    pub minutes: u64,
    /// Applis (nom de l'exécutable, `ms-teams.exe`) dont la WebView n'est jamais fermée.
    pub exclus: Vec<String>,
}

impl Default for Reglages {
    fn default() -> Self {
        Reglages {
            actif: true,
            minutes: 10,
            exclus: Vec::new(),
        }
    }
}

impl Reglages {
    pub const FICHIER: &'static str = "webview.json";

    pub fn charger(dir: &Path) -> Reglages {
        std::fs::read(dir.join(Self::FICHIER))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn enregistrer(&self, dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let json = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(dir.join(Self::FICHIER), json).map_err(|e| e.to_string())
    }
}

/// Une appli et ses WebView, telles que vues dans un relevé.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Groupe {
    pub owner: ProcId,
    pub owner_name: String,
    /// Processus WebView racines (enfants directs de l'appli) : les fermer ferme tout
    /// leur arbre.
    pub roots: Vec<ProcId>,
    /// Mémoire de travail de toutes ses WebView, en octets.
    pub bytes: u64,
    pub windowed: bool,
}

/// Appli propriétaire : premier ancêtre qui n'est pas une WebView, créé avant elle.
fn owner_of<'a>(p: &'a ProcInfo, by_pid: &HashMap<u32, &'a ProcInfo>) -> Option<&'a ProcInfo> {
    let mut cur = p;
    for _ in 0..16 {
        let parent = by_pid.get(&cur.parent).copied()?;
        if parent.id.pid == cur.id.pid || parent.id.created > cur.id.created {
            return None; // PID réutilisé : ce n'est pas son parent
        }
        if parent.name != WEBVIEW {
            return Some(parent);
        }
        cur = parent;
    }
    None
}

/// WebView de la session de l'utilisateur, regroupées par appli.
pub fn groupes(snap: &Snapshot) -> Vec<Groupe> {
    let by_pid: HashMap<u32, &ProcInfo> = snap.procs.iter().map(|p| (p.id.pid, p)).collect();
    let mut map: HashMap<ProcId, Groupe> = HashMap::new();
    for p in snap
        .procs
        .iter()
        .filter(|p| p.name == WEBVIEW && p.session == snap.user_session)
    {
        let Some(owner) = owner_of(p, &by_pid) else { continue };
        let g = map.entry(owner.id).or_insert_with(|| Groupe {
            owner: owner.id,
            owner_name: owner.name.clone(),
            roots: Vec::new(),
            bytes: 0,
            windowed: snap.windowed.contains(&owner.id.pid) || snap.foreground_pid == Some(owner.id.pid),
        });
        g.bytes += p.working_set;
        if by_pid.get(&p.parent).is_some_and(|parent| parent.id == owner.id) {
            g.roots.push(p.id);
        }
    }
    let mut out: Vec<Groupe> = map.into_values().collect();
    out.sort_by_key(|g| std::cmp::Reverse(g.bytes));
    out
}

/// Décision d'un passage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    /// Fermer les WebView de cette appli.
    Fermer(Groupe),
    /// L'appli a recréé sa WebView juste après qu'on l'a fermée (ou a redémarré pour
    /// ça, comme Teams) : la fermer ne rapporte rien. À ajouter aux exclusions.
    Recreee { name: String, bytes: u64 },
}

/// Après une fermeture, une WebView revenue dans ce délai rend la fermeture inutile.
pub const RECREATION_SECS: u64 = 300;

/// Suivi d'un passage à l'autre, par nom d'appli : une appli qui redémarre pour
/// recréer sa WebView change de PID, pas de nom.
#[derive(Debug, Default)]
pub struct Reaper {
    /// Secondes passées sans fenêtre, par appli.
    windowless: HashMap<String, u64>,
    /// Applis dont on a fermé la WebView, et depuis combien de secondes. Une appli n'est
    /// plus touchée tant que l'utilisateur ne l'a pas rouverte.
    closed: HashMap<String, u64>,
    /// Applis qui recréent leur WebView : plus jamais touchées (aussi ajoutées aux
    /// exclusions de l'utilisateur par le moteur, pour survivre à un redémarrage).
    useless: HashSet<String>,
}

impl Reaper {
    pub fn tick(&mut self, snap: &Snapshot, cfg: &Config, reglages: &Reglages, elapsed_secs: u64) -> Vec<Decision> {
        let gs = groupes(snap);
        for secs in self.closed.values_mut() {
            *secs += elapsed_secs;
        }
        let present: HashSet<&str> = gs.iter().map(|g| g.owner_name.as_str()).collect();
        self.windowless.retain(|k, _| present.contains(k.as_str()));
        if !reglages.actif {
            self.windowless.clear();
            return Vec::new();
        }
        let by_id: HashMap<ProcId, &ProcInfo> = snap.procs.iter().map(|p| (p.id, p)).collect();
        let mut out = Vec::new();
        for g in gs {
            let name = g.owner_name.clone();
            if self.useless.contains(&name) || reglages.exclus.iter().any(|e| e.eq_ignore_ascii_case(&name)) {
                continue;
            }
            if g.windowed {
                // Rouverte : elle a de nouveau droit à une fermeture plus tard.
                self.windowless.insert(name.clone(), 0);
                self.closed.remove(&name);
                continue;
            }
            if let Some(&since) = self.closed.get(&name) {
                if since <= RECREATION_SECS && !g.roots.is_empty() {
                    self.closed.remove(&name);
                    self.useless.insert(name.clone());
                    out.push(Decision::Recreee { name, bytes: g.bytes });
                }
                continue;
            }
            let secs = self.windowless.entry(name.clone()).or_insert(0);
            *secs += elapsed_secs;
            if *secs < reglages.minutes * 60 || g.roots.is_empty() {
                continue;
            }
            let Some(owner) = by_id.get(&g.owner) else { continue };
            if classify(owner, snap, cfg) != Class::Background {
                continue;
            }
            self.closed.insert(name, 0);
            out.push(Decision::Fermer(g));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classify::tests::proc;

    fn child(pid: u32, name: &str, parent: u32, mb: u64) -> ProcInfo {
        let mut p = proc(pid, name, None);
        p.parent = parent;
        p.working_set = mb << 20;
        p
    }

    fn snap(procs: Vec<ProcInfo>, windowed: Vec<u32>) -> Snapshot {
        Snapshot {
            procs,
            user_session: 1,
            self_pid: 1,
            windowed,
            ..Default::default()
        }
    }

    /// Teams (pid 100) avec une WebView (200) et deux moteurs de rendu (201, 202).
    fn teams() -> Vec<ProcInfo> {
        vec![
            proc(
                100,
                "ms-teams.exe",
                Some(r"c:\program files\windowsapps\msteams\ms-teams.exe"),
            ),
            child(200, WEBVIEW, 100, 120),
            child(201, WEBVIEW, 200, 90),
            child(202, WEBVIEW, 200, 60),
        ]
    }

    fn cfg() -> Config {
        Config::builtin()
    }

    #[test]
    fn webviews_are_grouped_under_their_app_with_their_memory() {
        let gs = groupes(&snap(teams(), vec![]));
        assert_eq!(gs.len(), 1);
        assert_eq!(gs[0].owner_name, "ms-teams.exe");
        assert_eq!(gs[0].roots.len(), 1);
        assert_eq!(gs[0].roots[0].pid, 200);
        assert_eq!(gs[0].bytes, 270 << 20);
    }

    #[test]
    fn closed_after_the_delay_without_window_and_only_once() {
        let mut r = Reaper::default();
        let s = snap(teams(), vec![]);
        let reg = Reglages::default();
        for _ in 0..59 {
            assert!(r.tick(&s, &cfg(), &reg, 10).is_empty());
        }
        let out = r.tick(&s, &cfg(), &reg, 10);
        assert!(matches!(&out[..], [Decision::Fermer(g)] if g.roots[0].pid == 200));
        // Fermée, et rien de recréé : on n'y revient pas.
        let closed = snap(vec![teams()[0].clone()], vec![]);
        for _ in 0..200 {
            assert!(r.tick(&closed, &cfg(), &reg, 10).is_empty());
        }
    }

    #[test]
    fn an_app_that_restarts_to_recreate_its_webview_is_left_alone() {
        // Teams, mesuré en VM : sa WebView fermée, il redémarre (nouveau PID) avec une
        // nouvelle WebView. Prism l'avait refermée 7 fois de suite.
        let mut r = Reaper::default();
        let reg = Reglages {
            minutes: 0,
            ..Default::default()
        };
        assert_eq!(r.tick(&snap(teams(), vec![]), &cfg(), &reg, 10).len(), 1);
        let mut again = teams();
        for (i, p) in again.iter_mut().enumerate() {
            p.id.pid += 1000;
            p.id.created += 50_000;
            if i > 0 {
                p.parent += 1000;
            }
        }
        let out = r.tick(&snap(again.clone(), vec![]), &cfg(), &reg, 10);
        assert!(matches!(&out[..], [Decision::Recreee { name, .. }] if name == "ms-teams.exe"));
        for _ in 0..100 {
            assert!(r.tick(&snap(again.clone(), vec![]), &cfg(), &reg, 10).is_empty());
        }
    }

    #[test]
    fn a_visible_window_resets_the_delay() {
        let mut r = Reaper::default();
        let reg = Reglages::default();
        let hidden = snap(teams(), vec![]);
        let shown = snap(teams(), vec![100]);
        for _ in 0..50 {
            r.tick(&hidden, &cfg(), &reg, 10);
        }
        r.tick(&shown, &cfg(), &reg, 10);
        for _ in 0..50 {
            assert!(r.tick(&hidden, &cfg(), &reg, 10).is_empty());
        }
    }

    #[test]
    fn games_excluded_apps_and_a_disabled_setting_are_never_touched() {
        let reg = Reglages {
            minutes: 0,
            ..Default::default()
        };
        // Un jeu (dossier de jeux) qui embarque une WebView.
        let game = vec![
            proc(100, "game.exe", Some(r"c:\xboxgames\fake\content\game.exe")),
            child(200, WEBVIEW, 100, 100),
        ];
        assert!(Reaper::default().tick(&snap(game, vec![]), &cfg(), &reg, 10).is_empty());
        let excl = Reglages {
            exclus: vec!["MS-Teams.exe".into()],
            ..reg.clone()
        };
        assert!(Reaper::default()
            .tick(&snap(teams(), vec![]), &cfg(), &excl, 10)
            .is_empty());
        let off = Reglages { actif: false, ..reg };
        assert!(Reaper::default()
            .tick(&snap(teams(), vec![]), &cfg(), &off, 10)
            .is_empty());
    }

    #[test]
    fn a_reused_parent_pid_is_not_taken_for_the_owner() {
        // Le parent a disparu et son PID a été repris par un processus plus récent.
        let mut owner = proc(100, "notepad.exe", None);
        owner.id.created = 5_000;
        let mut wv = child(200, WEBVIEW, 100, 100);
        wv.id.created = 1_000;
        assert!(groupes(&snap(vec![owner, wv], vec![])).is_empty());
    }
}
