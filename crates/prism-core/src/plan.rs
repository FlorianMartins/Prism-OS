//! Ce que Prism va faire, décidé à partir d'un relevé. Aucune action ici : le plan est
//! une liste ordonnée que la plateforme exécute.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::classify::{classify_all, Class};
use crate::config::{Config, Profile};
use crate::model::{MemPriority, PowerPlan, Priority, ProcId, PurgeScope, Snapshot, Target};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Action {
    Priority { target: Target, to: Priority },
    EcoQos { target: Target },
    MemoryPriority { target: Target, to: MemPriority },
    TrimWorkingSet { target: Target },
    PurgeStandby { scope: PurgeScope },
    HighPerformancePower,
    ShutdownWsl,
}

impl Action {
    pub fn describe(&self) -> String {
        match self {
            Action::Priority { target, to } => format!("{} : priorité CPU -> {to:?}", target.name),
            Action::EcoQos { target } => format!("{} : EcoQoS activé", target.name),
            Action::MemoryPriority { target, to } => format!("{} : priorité mémoire -> {to:?}", target.name),
            Action::TrimWorkingSet { target } => format!("{} : mémoire de travail rognée", target.name),
            Action::PurgeStandby { scope } => format!("purge du cache en attente ({scope:?})"),
            Action::HighPerformancePower => "plan d'alimentation -> performances élevées".into(),
            Action::ShutdownWsl => "arrêt de WSL (libère vmmem)".into(),
        }
    }
}

/// Noms du processus de la VM WSL2 selon les versions.
const WSL_VM_PROCESSES: [&str; 2] = ["vmmemwsl", "vmmem"];

/// Plan d'entrée en Mode Jeu.
///
/// `handled` : processus déjà traités dans cette session de jeu (on ne les retraite
/// pas). `first` : premier passage de la session — c'est le seul où l'on rogne, purge,
/// change l'alimentation et arrête WSL.
pub fn plan_engage(
    snap: &Snapshot,
    cfg: &Config,
    profile: &Profile,
    handled: &HashSet<ProcId>,
    first: bool,
) -> Vec<Action> {
    let background: Vec<Target> = classify_all(snap, cfg)
        .into_iter()
        .filter(|(p, c)| *c == Class::Background && !handled.contains(&p.id))
        .map(|(p, _)| Target::of(p))
        .collect();

    let mut actions = Vec::new();
    // 1. Réglages réversibles d'abord, priorité mémoire comprise : elle doit être en
    //    place AVANT le rognage pour que les pages rognées tombent dans le cache basse
    //    priorité (spec §4).
    for t in &background {
        if profile.background_priority != Priority::Normal {
            actions.push(Action::Priority {
                target: t.clone(),
                to: profile.background_priority,
            });
        }
        if profile.background_ecoqos {
            actions.push(Action::EcoQos { target: t.clone() });
        }
        if profile.background_memory_priority != MemPriority::Normal {
            actions.push(Action::MemoryPriority {
                target: t.clone(),
                to: profile.background_memory_priority,
            });
        }
    }
    if !first {
        return actions;
    }
    // 2. Rognage.
    if profile.trim_background {
        for t in &background {
            actions.push(Action::TrimWorkingSet { target: t.clone() });
        }
    }
    // 3. Purge, seulement si la RAM libre est sous le seuil.
    if should_purge(snap, profile) {
        actions.push(Action::PurgeStandby {
            scope: profile.purge_standby,
        });
    }
    if profile.power_plan == PowerPlan::HighPerformance {
        actions.push(Action::HighPerformancePower);
    }
    if profile.shutdown_wsl && wsl_running(snap) {
        actions.push(Action::ShutdownWsl);
    }
    actions
}

pub fn should_purge(snap: &Snapshot, profile: &Profile) -> bool {
    profile.purge_standby != PurgeScope::Off && snap.mem.free_percent() < profile.purge_when_free_below_percent
}

pub fn wsl_running(snap: &Snapshot) -> bool {
    snap.procs.iter().any(|p| WSL_VM_PROCESSES.contains(&p.name.as_str()))
}

/// Plan du nettoyage manuel (`prism ram clean`) : même séquence que le Mode Jeu côté
/// RAM, sans seuil (l'utilisateur l'a demandé). La priorité mémoire est remise à sa
/// valeur d'origine juste après par le moteur.
pub fn plan_clean(snap: &Snapshot, cfg: &Config, deep: bool) -> Vec<Action> {
    let background: Vec<Target> = classify_all(snap, cfg)
        .into_iter()
        .filter(|(_, c)| *c == Class::Background)
        .map(|(p, _)| Target::of(p))
        .collect();
    let mut actions: Vec<Action> = background
        .iter()
        .map(|t| Action::MemoryPriority {
            target: t.clone(),
            to: MemPriority::Low,
        })
        .collect();
    actions.extend(background.iter().map(|t| Action::TrimWorkingSet { target: t.clone() }));
    actions.push(Action::PurgeStandby {
        scope: if deep { PurgeScope::All } else { PurgeScope::Low },
    });
    actions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classify::tests::proc;
    use crate::model::MemStatus;

    const STEAM: &str = r"c:\steam\steamapps\common\elden ring\game\eldenring.exe";

    fn snap(free_percent: u64) -> Snapshot {
        Snapshot {
            procs: vec![
                proc(10, "eldenring.exe", Some(STEAM)),
                proc(11, "winword.exe", None),
                proc(12, "chrome.exe", None),
                proc(13, "discord.exe", None),
                proc(14, "vgc.exe", None),
                proc(15, "vmmemwsl", None),
            ],
            mem: MemStatus {
                total: 100,
                free: free_percent,
                ..Default::default()
            },
            user_session: 1,
            self_pid: 999,
        }
    }

    fn targets(actions: &[Action]) -> HashSet<String> {
        actions
            .iter()
            .filter_map(|a| match a {
                Action::Priority { target, .. }
                | Action::EcoQos { target }
                | Action::MemoryPriority { target, .. }
                | Action::TrimWorkingSet { target } => Some(target.name.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn only_background_is_touched() {
        let cfg = Config::builtin();
        let a = plan_engage(&snap(10), &cfg, cfg.profile("gaming").unwrap(), &HashSet::new(), true);
        let t = targets(&a);
        assert!(t.contains("winword.exe") && t.contains("chrome.exe"));
        for untouched in ["eldenring.exe", "discord.exe", "vgc.exe"] {
            assert!(!t.contains(untouched), "{untouched} ne doit jamais être visé");
        }
    }

    #[test]
    fn memory_priority_is_lowered_before_any_trim_and_purge_comes_last_of_ram() {
        let cfg = Config::builtin();
        let a = plan_engage(&snap(10), &cfg, cfg.profile("gaming").unwrap(), &HashSet::new(), true);
        let last_mem = a
            .iter()
            .rposition(|x| matches!(x, Action::MemoryPriority { .. }))
            .unwrap();
        let first_trim = a
            .iter()
            .position(|x| matches!(x, Action::TrimWorkingSet { .. }))
            .unwrap();
        let purge = a.iter().position(|x| matches!(x, Action::PurgeStandby { .. })).unwrap();
        assert!(last_mem < first_trim && first_trim < purge);
        assert!(a.contains(&Action::PurgeStandby { scope: PurgeScope::Low }));
        assert!(a.contains(&Action::HighPerformancePower));
        assert!(a.contains(&Action::ShutdownWsl));
    }

    #[test]
    fn no_purge_when_enough_free_ram() {
        let cfg = Config::builtin();
        let a = plan_engage(&snap(60), &cfg, cfg.profile("gaming").unwrap(), &HashSet::new(), true);
        assert!(!a.iter().any(|x| matches!(x, Action::PurgeStandby { .. })));
        assert!(
            a.iter().any(|x| matches!(x, Action::TrimWorkingSet { .. })),
            "le rognage reste fait"
        );
    }

    #[test]
    fn later_passes_only_handle_new_processes_and_never_trim() {
        let cfg = Config::builtin();
        let s = snap(10);
        let handled: HashSet<ProcId> = s
            .procs
            .iter()
            .filter(|p| p.name == "winword.exe")
            .map(|p| p.id)
            .collect();
        let a = plan_engage(&s, &cfg, cfg.profile("gaming").unwrap(), &handled, false);
        let t = targets(&a);
        assert!(t.contains("chrome.exe") && !t.contains("winword.exe"));
        assert!(a.iter().all(|x| !matches!(
            x,
            Action::TrimWorkingSet { .. }
                | Action::PurgeStandby { .. }
                | Action::ShutdownWsl
                | Action::HighPerformancePower
        )));
    }

    #[test]
    fn balanced_profile_leaves_ram_and_power_alone() {
        let cfg = Config::builtin();
        let a = plan_engage(&snap(5), &cfg, cfg.profile("balanced").unwrap(), &HashSet::new(), true);
        assert!(a
            .iter()
            .all(|x| matches!(x, Action::Priority { .. } | Action::EcoQos { .. })));
    }

    #[test]
    fn clean_is_ram_only_and_deep_purges_everything() {
        let cfg = Config::builtin();
        let a = plan_clean(&snap(50), &cfg, false);
        assert_eq!(a.last(), Some(&Action::PurgeStandby { scope: PurgeScope::Low }));
        assert!(!targets(&a).contains("eldenring.exe"));
        let deep = plan_clean(&snap(50), &cfg, true);
        assert_eq!(deep.last(), Some(&Action::PurgeStandby { scope: PurgeScope::All }));
    }
}
