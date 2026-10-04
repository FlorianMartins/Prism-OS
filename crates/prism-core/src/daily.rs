//! Mode Quotidien : Prism allège Windows en permanence, pas seulement en jeu.
//!
//! Une application d'arrière-plan **inactive** (aucun temps processeur notable, pas au
//! premier plan) est allégée par étapes :
//! 1. après `daily_eco_after_minutes` : EcoQoS, priorité mémoire basse, cœurs économes ;
//! 2. après `daily_trim_after_minutes` : sa mémoire de travail est rendue (le cas de
//!    Word resté ouvert toute la journée), et la RAM est surveillée.
//!
//! Dès que l'application redevient active, ou passe au premier plan, ses réglages sont
//! **annulés aussitôt** : on ne ralentit jamais ce dont l'utilisateur se sert.
//!
//! Pendant une partie, le Mode Jeu prend la main : le Mode Quotidien n'allège plus de
//! nouvelle appli (le Mode Jeu le fait plus fort), mais rend toujours la main à une appli
//! que l'utilisateur ramène au premier plan.

use std::collections::{HashMap, HashSet};

use crate::classify::{classify_all, Class};
use crate::config::{Config, Profile};
use crate::cores;
use crate::engine::{watch_ram_below, Report};
use crate::journal::{Journal, JournalStore, Undo};
use crate::model::{MemPriority, ProcId, Snapshot, Target};
use crate::plan::Action;
use crate::platform::{Outcome, Platform};

/// Une appli qui consomme plus de 1 % d'un cœur sur l'intervalle est « active ».
/// Temps processeur en unités de 100 ns : 1 % d'un cœur sur 1 s = 100 000.
const ACTIVE_PER_SECOND: u64 = 100_000;

/// Au plus une purge de surveillance toutes les N passes.
const PURGE_EVERY_POLLS: u32 = 15;

#[derive(Clone, Debug, Default)]
struct Track {
    last_cpu: u64,
    idle_secs: u64,
    eased: bool,
    trimmed: bool,
}

#[derive(Default)]
pub struct Daily {
    tracks: HashMap<ProcId, Track>,
    pub journal: Journal,
    since_purge: u32,
}

fn target_of(u: &Undo) -> Option<ProcId> {
    match u {
        Undo::Priority { target, .. }
        | Undo::EcoQos { target, .. }
        | Undo::MemoryPriority { target, .. }
        | Undo::CpuSets { target, .. }
        | Undo::Resume { target } => Some(target.id),
        Undo::PowerPlan { .. } | Undo::Service { .. } => None,
    }
}

impl Daily {
    pub fn eased(&self) -> usize {
        self.tracks.values().filter(|t| t.eased).count()
    }

    fn save(&self, store: &mut dyn JournalStore, report: &mut Report) {
        let res = if self.journal.entries.is_empty() {
            store.clear()
        } else {
            store.save(&self.journal)
        };
        if let Err(e) = res {
            report.failed.push(format!("journal quotidien non écrit : {e}"));
        }
    }

    /// Annule tout ce que le Mode Quotidien a fait à ce processus.
    fn revert(&mut self, platform: &mut dyn Platform, id: ProcId, report: &mut Report) {
        let (mine, rest): (Vec<Undo>, Vec<Undo>) = std::mem::take(&mut self.journal.entries)
            .into_iter()
            .partition(|u| target_of(u) == Some(id));
        self.journal.entries = rest;
        for u in mine.iter().rev() {
            let outcome = platform.undo(u);
            report.record(format!("rendu : {}", u.describe()), &outcome);
        }
    }

    /// Un passage. `elapsed_secs` : temps écoulé depuis le précédent ;
    /// `game_running` : une session de Mode Jeu est ouverte.
    #[allow(clippy::too_many_arguments)]
    pub fn tick(
        &mut self,
        platform: &mut dyn Platform,
        store: &mut dyn JournalStore,
        cfg: &Config,
        profile: &Profile,
        snap: &Snapshot,
        elapsed_secs: u64,
        game_running: bool,
    ) -> Report {
        let mut report = Report::default();
        if !profile.daily {
            if !self.journal.entries.is_empty() {
                let journal = std::mem::take(&mut self.journal);
                for u in journal.entries.iter().rev() {
                    let outcome = platform.undo(u);
                    report.record(format!("rendu : {}", u.describe()), &outcome);
                }
                self.save(store, &mut report);
            }
            self.tracks.clear();
            return report;
        }

        let background: Vec<_> = classify_all(snap, cfg)
            .into_iter()
            .filter(|(_, c)| *c == Class::Background)
            .map(|(p, _)| p)
            .collect();
        let alive: HashSet<ProcId> = background.iter().map(|p| p.id).collect();

        // Processus fermés : on oublie leur suivi et leurs entrées (rien à annuler).
        let before = self.journal.entries.len();
        self.tracks.retain(|id, _| alive.contains(id));
        self.journal
            .entries
            .retain(|u| target_of(u).map_or(true, |id| alive.contains(&id)));
        let mut dirty = self.journal.entries.len() != before;

        let split = if profile.background_cpu_sets {
            cores::split(&snap.cpus)
        } else {
            None
        };
        let eco_after = profile.daily_eco_after_minutes * 60;
        let trim_after = profile.daily_trim_after_minutes * 60;
        let mut trimmed_now = false;

        for p in background {
            let Some(track) = self.tracks.get_mut(&p.id) else {
                self.tracks.insert(
                    p.id,
                    Track {
                        last_cpu: p.cpu_time,
                        ..Default::default()
                    },
                );
                continue;
            };
            let delta = p.cpu_time.saturating_sub(track.last_cpu);
            track.last_cpu = p.cpu_time;
            let foreground = snap.foreground_pid == Some(p.id.pid);
            let busy = delta > elapsed_secs.max(1) * ACTIVE_PER_SECOND;

            if foreground || busy {
                track.idle_secs = 0;
                track.trimmed = false;
                // En jeu, une appli qui travaille seule reste allégée ; une appli que
                // l'utilisateur ramène au premier plan lui est rendue tout de suite.
                if track.eased && (foreground || !game_running) {
                    track.eased = false;
                    self.revert(platform, p.id, &mut report);
                    dirty = true;
                }
                continue;
            }
            track.idle_secs += elapsed_secs;

            if !track.eased && !game_running && track.idle_secs >= eco_after {
                track.eased = true;
                let t = Target::of(p);
                let mut actions = vec![
                    Action::EcoQos { target: t.clone() },
                    Action::MemoryPriority {
                        target: t.clone(),
                        to: MemPriority::Low,
                    },
                ];
                if let Some(s) = &split {
                    actions.push(Action::CpuSets {
                        target: t,
                        cpus: s.background.clone(),
                    });
                }
                for a in &actions {
                    let outcome = platform.apply(a);
                    if let Outcome::Done(Some(u)) = &outcome {
                        self.journal.entries.push(u.clone());
                        dirty = true;
                    }
                    report.record(format!("inactif : {}", a.describe()), &outcome);
                }
            }
            if track.eased && !track.trimmed && track.idle_secs >= trim_after {
                track.trimmed = true;
                trimmed_now = true;
                let a = Action::TrimWorkingSet { target: Target::of(p) };
                let outcome = platform.apply(&a);
                report.record(format!("inactif : {}", a.describe()), &outcome);
            }
        }
        if dirty {
            self.save(store, &mut report);
        }

        self.since_purge += 1;
        if profile.daily_purge_below_percent > 0 && (trimmed_now || self.since_purge >= PURGE_EVERY_POLLS) {
            if let Some(r) = watch_ram_below(platform, profile.daily_purge_below_percent, snap, trimmed_now) {
                self.since_purge = 0;
                report.merge(r);
            }
        }
        report
    }

    /// Fin de la surveillance : tout est rendu.
    pub fn release(&mut self, platform: &mut dyn Platform, store: &mut dyn JournalStore) -> Report {
        let mut report = Report::default();
        let journal = std::mem::take(&mut self.journal);
        for u in journal.entries.iter().rev() {
            let outcome = platform.undo(u);
            report.record(format!("rendu : {}", u.describe()), &outcome);
        }
        self.tracks.clear();
        self.save(store, &mut report);
        report
    }
}
