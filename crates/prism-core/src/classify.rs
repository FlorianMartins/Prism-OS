//! Classement des processus (spec v0.1 §2). L'ordre des règles est la garantie de
//! sécurité : ce qui est protégé l'est avant toute autre considération.

use crate::config::{Config, GamingConflict};
use crate::glob::any_matches;
use crate::model::{ProcInfo, Snapshot};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Class {
    /// Jamais touché.
    Protected,
    /// Outil gênant pour les anti-cheats : pas touché, mais signalé.
    Conflict { tool: String },
    /// Le jeu : jamais touché.
    Game,
    /// Utile pendant le jeu (voix, capture, overlay) : pas touché.
    Companion,
    /// Mis en retrait pendant le jeu.
    Background,
}

impl Class {
    pub fn label(&self) -> &'static str {
        match self {
            Class::Protected => "protégé",
            Class::Conflict { .. } => "conflit anti-cheat",
            Class::Game => "jeu",
            Class::Companion => "compagnon",
            Class::Background => "arrière-plan",
        }
    }
}

pub fn classify(p: &ProcInfo, snap: &Snapshot, cfg: &Config) -> Class {
    let lists = &cfg.lists;
    if p.id.pid == snap.self_pid || p.id.pid <= 4 || p.session != snap.user_session {
        return Class::Protected;
    }
    if any_matches(&lists.protected, &p.name) {
        return Class::Protected;
    }
    if let Some(tool) = conflict_tool(p, cfg) {
        return Class::Conflict { tool };
    }
    if is_game(p, cfg) {
        return Class::Game;
    }
    if any_matches(&lists.companions, &p.name) {
        return Class::Companion;
    }
    Class::Background
}

fn conflict_tool(p: &ProcInfo, cfg: &Config) -> Option<String> {
    cfg.tools
        .iter()
        .filter(|t| t.gaming_conflict == GamingConflict::Warn)
        .find(|t| any_matches(&t.processes, &p.name))
        .map(|t| t.id.clone())
}

fn is_game(p: &ProcInfo, cfg: &Config) -> bool {
    let lists = &cfg.lists;
    if any_matches(&lists.games, &p.name) {
        return true;
    }
    let Some(path) = &p.path else { return false };
    let under_root = lists.game_roots.iter().any(|root| path.contains(root.as_str()));
    under_root && !any_matches(&lists.game_helpers, &p.name)
}

/// Les processus de la session, avec leur classe.
pub fn classify_all<'a>(snap: &'a Snapshot, cfg: &Config) -> Vec<(&'a ProcInfo, Class)> {
    snap.procs.iter().map(|p| (p, classify(p, snap, cfg))).collect()
}

pub fn games_running(snap: &Snapshot, cfg: &Config) -> Vec<String> {
    let mut names: Vec<String> = classify_all(snap, cfg)
        .into_iter()
        .filter(|(_, c)| *c == Class::Game)
        .map(|(p, _)| p.name.clone())
        .collect();
    names.sort();
    names.dedup();
    names
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::model::ProcId;

    pub fn proc(pid: u32, name: &str, path: Option<&str>) -> ProcInfo {
        ProcInfo {
            id: ProcId {
                pid,
                created: 1000 + pid as u64,
            },
            name: name.into(),
            path: path.map(Into::into),
            session: 1,
            working_set: 100 << 20,
            cpu_time: 0,
        }
    }

    fn snap(procs: Vec<ProcInfo>) -> Snapshot {
        Snapshot {
            procs,
            user_session: 1,
            self_pid: 999,
            ..Default::default()
        }
    }

    fn class_of(p: ProcInfo) -> Class {
        let s = snap(vec![p.clone()]);
        classify(&p, &s, &Config::builtin())
    }

    #[test]
    fn steam_game_is_a_game_but_its_crash_handler_is_not() {
        let root = r"c:\program files (x86)\steam\steamapps\common\cs2\game\bin\win64\";
        assert_eq!(
            class_of(proc(10, "cs2.exe", Some(&format!("{root}cs2.exe")))),
            Class::Game
        );
        assert_eq!(
            class_of(proc(
                11,
                "crashpad_handler.exe",
                Some(&format!("{root}crashpad_handler.exe"))
            )),
            Class::Background
        );
    }

    #[test]
    fn anticheat_under_a_game_root_stays_protected() {
        let p = proc(
            12,
            "easyanticheat_eos.exe",
            Some(r"c:\program files (x86)\steam\steamapps\common\game\easyanticheat\easyanticheat_eos.exe"),
        );
        assert_eq!(class_of(p), Class::Protected, "protégé passe avant jeu");
    }

    #[test]
    fn self_other_sessions_and_system_are_protected() {
        let cfg = Config::builtin();
        let mut other = proc(20, "word.exe", None);
        other.session = 0;
        let me = proc(999, "prism.exe", None);
        let system = proc(4, "system", None);
        let s = snap(vec![other.clone(), me.clone(), system.clone()]);
        for p in [&other, &me, &system] {
            assert_eq!(classify(p, &s, &cfg), Class::Protected, "{}", p.name);
        }
    }

    #[test]
    fn companions_and_background() {
        assert_eq!(class_of(proc(30, "discord.exe", None)), Class::Companion);
        assert_eq!(class_of(proc(31, "obs64.exe", None)), Class::Companion);
        assert_eq!(class_of(proc(32, "winword.exe", None)), Class::Background);
        assert_eq!(class_of(proc(33, "chrome.exe", None)), Class::Background);
    }

    #[test]
    fn debugger_is_a_conflict() {
        assert_eq!(
            class_of(proc(40, "x64dbg.exe", None)),
            Class::Conflict { tool: "x64dbg".into() }
        );
        assert_eq!(
            class_of(proc(41, "wireshark.exe", None)),
            Class::Background,
            "pas de conflit déclaré"
        );
    }

    #[test]
    fn explicit_game_names_count() {
        let mut cfg = Config::builtin();
        cfg.lists.games.push("mygame.exe".into());
        let p = proc(50, "mygame.exe", Some(r"d:\jeux\mygame.exe"));
        let s = snap(vec![p.clone()]);
        assert_eq!(classify(&p, &s, &cfg), Class::Game);
        assert_eq!(games_running(&s, &cfg), vec!["mygame.exe".to_string()]);
    }
}
