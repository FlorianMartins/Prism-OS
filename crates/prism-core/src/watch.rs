//! Boucle de surveillance : décide à chaque passage d'entrer en Mode Jeu, de traiter
//! les nouveaux processus, ou de restaurer.

use crate::classify::games_running;
use crate::config::{Config, Profile};
use crate::engine::{engage, restore, watch_ram, GameSession, Report};
use crate::journal::JournalStore;
use crate::model::Snapshot;
use crate::platform::Platform;
use crate::tools::{conflicts, Conflict};

#[derive(Debug, PartialEq, Eq)]
pub enum Event {
    Idle,
    Engaged {
        games: Vec<String>,
        report: Report,
        conflicts: Vec<Conflict>,
    },
    Updated {
        report: Report,
    },
    /// Plus de jeu, mais on attend encore quelques passages avant de restaurer.
    Cooling {
        remaining: u32,
    },
    Released {
        report: Report,
    },
}

#[derive(Default)]
pub struct Watcher {
    pub session: Option<GameSession>,
    /// Jeux de la partie en cours (pour l'état affiché).
    pub games: Vec<String>,
    quiet_polls: u32,
    /// Passages depuis la dernière purge de surveillance (limite la cadence).
    since_watch_purge: u32,
}

/// Au plus une purge de surveillance toutes les N passes (≈ 20 s à 2 s par passe).
pub const WATCH_EVERY_POLLS: u32 = 10;

impl Watcher {
    pub fn engaged(&self) -> bool {
        self.session.is_some()
    }

    pub fn tick(
        &mut self,
        platform: &mut dyn Platform,
        store: &mut dyn JournalStore,
        cfg: &Config,
        profile_name: &str,
        profile: &Profile,
        snap: &Snapshot,
    ) -> Event {
        let games = if profile.game_mode {
            games_running(snap, cfg)
        } else {
            Vec::new()
        };

        if !games.is_empty() {
            self.quiet_polls = 0;
            let first = self.session.is_none();
            self.games = games.clone();
            let report = engage(platform, store, cfg, profile_name, profile, &mut self.session, snap);
            return if first {
                Event::Engaged {
                    games,
                    report,
                    conflicts: conflicts(snap, cfg),
                }
            } else if report.done.is_empty() && report.failed.is_empty() {
                self.since_watch_purge += 1;
                if self.since_watch_purge >= WATCH_EVERY_POLLS {
                    if let Some(r) = watch_ram(platform, profile, snap) {
                        self.since_watch_purge = 0;
                        return Event::Updated { report: r };
                    }
                }
                Event::Idle
            } else {
                Event::Updated { report }
            };
        }

        if self.session.is_none() {
            return Event::Idle;
        }
        self.quiet_polls += 1;
        // Profil sans Mode Jeu (changé en cours de partie) : on restaure tout de suite.
        if profile.game_mode && self.quiet_polls < cfg.release_after_polls {
            return Event::Cooling {
                remaining: cfg.release_after_polls - self.quiet_polls,
            };
        }
        self.release(platform, store)
    }

    /// Restaure tout de suite (fin de la surveillance, Ctrl-C).
    pub fn release(&mut self, platform: &mut dyn Platform, store: &mut dyn JournalStore) -> Event {
        self.quiet_polls = 0;
        self.games.clear();
        match self.session.take() {
            Some(s) => Event::Released {
                report: restore(platform, store, &s.journal),
            },
            None => Event::Idle,
        }
    }
}
