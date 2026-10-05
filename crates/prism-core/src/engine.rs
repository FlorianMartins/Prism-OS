//! Exécution des plans : applique, journalise, restaure.

use std::collections::HashSet;

use crate::config::{Config, Profile};
use crate::journal::{Journal, JournalStore, MemStore, Undo};
use crate::model::{MemStatus, ProcId, Snapshot};
use crate::plan::{plan_clean, plan_engage, Action};
use crate::platform::{Outcome, Platform};

/// Compte rendu lisible d'un passage.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub done: Vec<String>,
    pub skipped: Vec<String>,
    pub failed: Vec<String>,
    pub mem_before: Option<MemStatus>,
    pub mem_after: Option<MemStatus>,
}

impl Report {
    pub(crate) fn record(&mut self, what: String, outcome: &Outcome) {
        match outcome {
            Outcome::Done(_) => self.done.push(what),
            Outcome::Skipped(why) => self.skipped.push(format!("{what} — {why}")),
            Outcome::Failed(why) => self.failed.push(format!("{what} — {why}")),
        }
    }

    pub fn merge(&mut self, other: Report) {
        self.done.extend(other.done);
        self.skipped.extend(other.skipped);
        self.failed.extend(other.failed);
        self.mem_before = self.mem_before.or(other.mem_before);
        self.mem_after = other.mem_after.or(self.mem_after);
    }
}

/// Une session de Mode Jeu en cours.
#[derive(Clone, Debug, Default)]
pub struct GameSession {
    pub journal: Journal,
    pub handled: HashSet<ProcId>,
}

fn action_target(a: &Action) -> Option<ProcId> {
    match a {
        Action::Priority { target, .. }
        | Action::EcoQos { target }
        | Action::MemoryPriority { target, .. }
        | Action::TrimWorkingSet { target }
        | Action::CpuSets { target, .. }
        | Action::Suspend { target } => Some(target.id),
        _ => None,
    }
}

/// Exécute `actions` dans l'ordre. Chaque état d'origine est écrit dans le journal
/// **avant** l'action suivante.
fn run(
    platform: &mut dyn Platform,
    store: &mut dyn JournalStore,
    session: &mut GameSession,
    actions: &[Action],
    report: &mut Report,
) {
    for action in actions {
        let outcome = platform.apply(action);
        if let Outcome::Done(Some(undo)) = &outcome {
            session.journal.entries.push(undo.clone());
            if let Err(e) = store.save(&session.journal) {
                report.failed.push(format!("journal non écrit : {e}"));
            }
        }
        if let Some(id) = action_target(action) {
            session.handled.insert(id);
        }
        report.record(action.describe(), &outcome);
    }
}

/// Entre en Mode Jeu (`session` vide) ou traite les nouveaux processus d'une session
/// déjà ouverte.
pub fn engage(
    platform: &mut dyn Platform,
    store: &mut dyn JournalStore,
    cfg: &Config,
    profile_name: &str,
    profile: &Profile,
    session: &mut Option<GameSession>,
    snap: &Snapshot,
) -> Report {
    let first = session.is_none();
    let s = session.get_or_insert_with(|| GameSession {
        journal: Journal {
            profile: profile_name.to_string(),
            entries: Vec::new(),
        },
        handled: HashSet::new(),
    });
    let actions = plan_engage(snap, cfg, profile, &s.handled, first);
    let mut report = Report {
        mem_before: Some(snap.mem),
        ..Default::default()
    };
    run(platform, store, s, &actions, &mut report);
    if first {
        report.mem_after = platform.snapshot().ok().map(|s| s.mem);
    }
    report
}

/// Annule les entrées du journal, de la plus récente à la plus ancienne, puis efface
/// le journal. Une entrée dont le processus a disparu est simplement ignorée.
pub fn restore(platform: &mut dyn Platform, store: &mut dyn JournalStore, journal: &Journal) -> Report {
    let mut report = Report::default();
    for undo in journal.entries.iter().rev() {
        let outcome = platform.undo(undo);
        report.record(undo.describe(), &outcome);
    }
    if let Err(e) = store.clear() {
        report.failed.push(format!("journal non effacé : {e}"));
    }
    report
}

/// Au démarrage : si un journal traîne (Prism tué en pleine partie), on restaure.
pub fn recover(platform: &mut dyn Platform, store: &mut dyn JournalStore) -> Result<Option<Report>, String> {
    match store.load()? {
        Some(journal) => Ok(Some(restore(platform, store, &journal))),
        None => Ok(None),
    }
}

/// Résultat d'un nettoyage, en mémoire **utilisée** (le chiffre du Gestionnaire des
/// tâches ; la mémoire « libre » ne dit rien, Windows garde tout en cache) : « Mémoire
/// utilisée : 9,9 → 7,4 Go (−2,5 Go) · 3 services arrêtés ». L'écart est calculé sur
/// les valeurs affichées (au dixième).
pub fn clean_summary(used_before: u64, used_after: u64, services: usize) -> String {
    const STEP: u64 = 1024 * 1024 * 1024 / 10;
    let round = |b: u64| (b + STEP / 2) / STEP;
    let gb = |tenths: u64| format!("{},{}", tenths / 10, tenths % 10);
    let (b, a) = (round(used_before), round(used_after));
    let mut s = format!("Mémoire utilisée : {} → {} Go", gb(b), gb(a));
    if b > a {
        s.push_str(&format!(" (−{} Go)", gb(b - a)));
    }
    match services {
        0 => {}
        1 => s.push_str(" · 1 service arrêté"),
        n => s.push_str(&format!(" · {n} services arrêtés")),
    }
    s
}

/// Nettoyage RAM manuel : priorité mémoire basse, rognage, purge, puis priorité
/// mémoire remise aussitôt à sa valeur d'origine.
pub fn clean(platform: &mut dyn Platform, cfg: &Config, deep: bool) -> Result<Report, String> {
    let snap = platform.snapshot()?;
    let actions = plan_clean(&snap, cfg, deep);
    let mut store = MemStore::default();
    let mut session = GameSession::default();
    let mut report = Report {
        mem_before: Some(snap.mem),
        ..Default::default()
    };
    run(platform, &mut store, &mut session, &actions, &mut report);
    let journal = std::mem::take(&mut session.journal);
    let undo_report = restore(platform, &mut store, &journal);
    report.failed.extend(undo_report.failed);
    report.mem_after = platform.snapshot().ok().map(|s| s.mem);
    Ok(report)
}

/// Vrai si le journal contient une entrée qui touche ce processus (tests, diagnostic).
pub fn journal_touches(journal: &Journal, id: ProcId) -> bool {
    journal.entries.iter().any(|u| match u {
        Undo::Priority { target, .. } | Undo::EcoQos { target, .. } | Undo::MemoryPriority { target, .. } => {
            target.id == id
        }
        Undo::CpuSets { target, .. } | Undo::Resume { target } => target.id == id,
        Undo::PowerPlan { .. } | Undo::Service { .. } => false,
    })
}

/// Seuil de cache libérable en dessous duquel la surveillance ne purge pas (le gain
/// ne vaut pas l'appel).
pub const WATCH_MIN_PURGEABLE: u64 = 256 * 1024 * 1024;

/// Surveillance de la RAM en pleine partie (Mode Jeu, seuil du profil).
pub fn watch_ram(platform: &mut dyn Platform, profile: &Profile, snap: &Snapshot) -> Option<Report> {
    if !profile.watch_ram || profile.purge_standby == crate::model::PurgeScope::Off {
        return None;
    }
    watch_ram_below(platform, profile.purge_when_free_below_percent, snap, false)
}

/// Si la RAM libre est sous `threshold_percent` et qu'il y a du cache de priorité 0 à
/// reprendre, on le purge. Seule la priorité 0 est visée : le cache des fichiers des
/// applis et du jeu n'est jamais touché. `just_trimmed` : des pages viennent d'être
/// rognées et attendent encore dans la liste modifiée (le relevé ne les montre pas
/// dans le cache) ; la purge Windows les écrit d'abord.
pub fn watch_ram_below(
    platform: &mut dyn Platform,
    threshold_percent: u64,
    snap: &Snapshot,
    just_trimmed: bool,
) -> Option<Report> {
    if snap.mem.free_percent() >= threshold_percent {
        return None;
    }
    if !just_trimmed && snap.mem.standby_low < WATCH_MIN_PURGEABLE {
        return None;
    }
    let action = Action::PurgeStandby {
        scope: crate::model::PurgeScope::Low,
    };
    let mut report = Report {
        mem_before: Some(snap.mem),
        ..Default::default()
    };
    let outcome = platform.apply(&action);
    report.record(format!("surveillance RAM : {}", action.describe()), &outcome);
    report.mem_after = platform.snapshot().ok().map(|s| s.mem);
    Some(report)
}

#[cfg(test)]
mod tests {
    use super::clean_summary;

    const GB: u64 = 1024 * 1024 * 1024;

    #[test]
    fn clean_summary_speaks_in_memory_used_and_counts_stopped_services() {
        assert_eq!(
            clean_summary(99 * GB / 10, 74 * GB / 10, 3),
            "Mémoire utilisée : 9,9 → 7,4 Go (−2,5 Go) · 3 services arrêtés"
        );
        // L'écart suit les valeurs affichées : 3,8 → 3,7 dit −0,1, pas −0,2.
        assert_eq!(
            clean_summary(3_840 * GB / 1000, 3_660 * GB / 1000, 1),
            "Mémoire utilisée : 3,8 → 3,7 Go (−0,1 Go) · 1 service arrêté"
        );
        assert_eq!(clean_summary(2 * GB, 2 * GB, 0), "Mémoire utilisée : 2,0 → 2,0 Go");
    }
}
