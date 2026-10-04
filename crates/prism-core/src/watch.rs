//! Boucle de surveillance : décide à chaque passage d'entrer en Mode Jeu, de traiter
//! les nouveaux processus, ou de restaurer.

use crate::classify::games_running;
use crate::config::{Config, Profile};
use crate::engine::{engage, restore, GameSession, Report};
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
    quiet_polls: u32,
}

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
            let report = engage(platform, store, cfg, profile_name, profile, &mut self.session, snap);
            return if first {
                Event::Engaged {
                    games,
                    report,
                    conflicts: conflicts(snap, cfg),
                }
            } else if report.done.is_empty() && report.failed.is_empty() {
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
        match self.session.take() {
            Some(s) => Event::Released {
                report: restore(platform, store, &s.journal),
            },
            None => Event::Idle,
        }
    }
}
