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
/// Au plus une purge du cache basse priorité toutes les 30 s (en secondes : le moteur
/// espace ses relevés hors partie, un compte en passages la rendait 5 fois plus rare).
const PURGE_EVERY_SECS: u64 = 30;

#[derive(Clone, Debug, Default)]
struct Track {
    last_cpu: u64,
    idle_secs: u64,
    eased: bool,
    trimmed: bool,
    /// Le rognage impute son temps noyau au processus rogné (mesuré en VM) : la
    /// mesure suivante ne dit rien de son activité réelle.
    skip_next: bool,
}

/// L'appli au premier plan entière : les processus du même exécutable (les onglets
/// d'un navigateur, les fenêtres d'Electron) et leurs descendants (une WebView lancée par
/// l'appli). Seul le processus principal possède la fenêtre ; ses onglets restaient
/// sinon « d'arrière-plan » pendant qu'on les lisait.
pub fn foreground_family(snap: &Snapshot) -> HashSet<u32> {
    let Some(fg) = snap.foreground_pid else {
        return HashSet::new();
    };
    let name = snap.procs.iter().find(|p| p.id.pid == fg).map(|p| p.name.as_str());
    let mut family: HashSet<u32> = snap
        .procs
        .iter()
        .filter(|p| p.id.pid == fg || name.is_some_and(|n| p.name.eq_ignore_ascii_case(n)))
        .map(|p| p.id.pid)
        .collect();
    // Descendants : on étend jusqu'à stabilité (les arbres de processus sont peu profonds).
    loop {
        let more: Vec<u32> = snap
            .procs
            .iter()
            .filter(|p| !family.contains(&p.id.pid) && p.parent != 0 && family.contains(&p.parent))
            .map(|p| p.id.pid)
            .collect();
        if more.is_empty() {
            return family;
        }
        family.extend(more);
    }
}

#[derive(Default)]
pub struct Daily {
    tracks: HashMap<ProcId, Track>,
    pub journal: Journal,
    since_purge: u64,
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

    /// Noms des applis allégées en ce moment (triés, sans doublon).
    pub fn eased_names(&self, snap: &Snapshot) -> Vec<String> {
        let mut names: Vec<String> = snap
            .procs
            .iter()
            .filter(|p| self.tracks.get(&p.id).is_some_and(|t| t.eased))
            .map(|p| p.name.clone())
            .collect();
        names.sort();
        names.dedup();
        names
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

        // Applis d'arrière-plan, et compagnons de jeu (Discord, Steam…) : ceux-ci rendent
        // leur RAM hors partie quand ils sont inactifs, sans être ralentis (pas
        // d'EcoQoS ni de cœurs économes : un vocal doit rester fluide), et sont rendus
        // dès qu'une partie commence. Avant, jamais touchés : les plus gros consommateurs
        // (rapport d'un PC réel : Discord 1,5 Go, Steam 1,15 Go) gardaient tout.
        let classified = classify_all(snap, cfg);
        let companions: HashSet<ProcId> = classified
            .iter()
            .filter(|(_, c)| *c == Class::Companion)
            .map(|(p, _)| p.id)
            .collect();
        let background: Vec<_> = classified
            .into_iter()
            .filter(|(_, c)| *c == Class::Background || *c == Class::Companion)
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
        let family = foreground_family(snap);
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
            if std::mem::take(&mut track.skip_next) {
                track.idle_secs += elapsed_secs;
                continue;
            }
            let foreground = family.contains(&p.id.pid);
            let busy = delta > elapsed_secs.max(1) * ACTIVE_PER_SECOND;
            let companion = companions.contains(&p.id);

            // Compagnon pendant une partie : rendu tout de suite, plus touché.
            if companion && game_running {
                track.idle_secs = 0;
                track.trimmed = false;
                if track.eased {
                    track.eased = false;
                    self.revert(platform, p.id, &mut report);
                    dirty = true;
                }
                continue;
            }

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
                let mut actions = vec![Action::MemoryPriority {
                    target: t.clone(),
                    to: MemPriority::Low,
                }];
                if !companion {
                    actions.insert(0, Action::EcoQos { target: t.clone() });
                }
                if let (Some(s), false) = (&split, companion) {
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
                track.skip_next = true;
                trimmed_now = true;
                let a = Action::TrimWorkingSet { target: Target::of(p) };
                let outcome = platform.apply(&a);
                report.record(format!("inactif : {}", a.describe()), &outcome);
            }
        }
        if dirty {
            self.save(store, &mut report);
        }

        self.since_purge += elapsed_secs;
        if profile.daily_purge_below_percent > 0 && (trimmed_now || self.since_purge >= PURGE_EVERY_SECS) {
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
