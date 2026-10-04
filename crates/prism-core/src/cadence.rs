//! Cadence du moteur : une vérification légère à chaque passe (la liste des PID, sans
//! ouvrir un seul processus) et le relevé complet (chemins, mémoire, temps CPU de
//! chaque processus) seulement quand il peut servir.
//!
//! Relevé complet :
//! - au premier passage ;
//! - quand un processus est apparu depuis la vérification précédente (ce peut être un
//!   jeu : le Mode Jeu s'engage aussi vite qu'avant) ;
//! - pendant une partie et son délai de restauration (le Mode Jeu suit tout) ;
//! - sinon, toutes les `full_every_secs` secondes (le Mode Quotidien n'a pas besoin de
//!   plus : il attend des minutes avant d'alléger une appli).

use std::collections::HashSet;

#[derive(Debug)]
pub struct Cadence {
    full_every_secs: u64,
    last_pids: Option<HashSet<u32>>,
    since_full_secs: u64,
    first: bool,
}

impl Cadence {
    pub fn new(full_every_secs: u64) -> Self {
        Self {
            full_every_secs,
            last_pids: None,
            since_full_secs: 0,
            first: true,
        }
    }

    /// Une passe de `elapsed_secs` secondes. `pids` : PID présents (`None` si la
    /// plateforme ne sait pas les lister à bas coût : relevé complet à chaque passe).
    /// `busy` : partie en cours ou délai de restauration. Rend `Some(secondes écoulées
    /// depuis le relevé complet précédent)` s'il faut en faire un maintenant.
    pub fn due(&mut self, pids: Option<Vec<u32>>, busy: bool, elapsed_secs: u64) -> Option<u64> {
        if !self.first {
            self.since_full_secs += elapsed_secs;
        }
        let appeared = match (pids, &self.last_pids) {
            (None, _) => true,
            (Some(now), prev) => {
                let now: HashSet<u32> = now.into_iter().collect();
                let new = prev.as_ref().map_or(true, |p| now.iter().any(|pid| !p.contains(pid)));
                self.last_pids = Some(now);
                new
            }
        };
        if self.first || appeared || busy || self.since_full_secs >= self.full_every_secs {
            self.first = false;
            let since = std::mem::take(&mut self.since_full_secs);
            return Some(since);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_pass_is_full() {
        let mut c = Cadence::new(10);
        assert_eq!(c.due(Some(vec![1, 2]), false, 2), Some(0));
    }

    #[test]
    fn nothing_new_waits_for_the_full_interval() {
        let mut c = Cadence::new(10);
        c.due(Some(vec![1, 2]), false, 2);
        for _ in 0..4 {
            assert_eq!(c.due(Some(vec![1, 2]), false, 2), None);
        }
        assert_eq!(c.due(Some(vec![1, 2]), false, 2), Some(10));
    }

    #[test]
    fn a_process_that_exits_does_not_trigger() {
        let mut c = Cadence::new(10);
        c.due(Some(vec![1, 2, 3]), false, 2);
        assert_eq!(c.due(Some(vec![1, 2]), false, 2), None);
    }

    #[test]
    fn a_new_process_triggers_at_once() {
        let mut c = Cadence::new(10);
        c.due(Some(vec![1, 2]), false, 2);
        assert_eq!(c.due(Some(vec![1, 2]), false, 2), None);
        assert_eq!(c.due(Some(vec![1, 2, 7]), false, 2), Some(4));
        // Vu une fois : ne redéclenche pas.
        assert_eq!(c.due(Some(vec![1, 2, 7]), false, 2), None);
    }

    #[test]
    fn during_a_game_every_pass_is_full() {
        let mut c = Cadence::new(10);
        c.due(Some(vec![1]), false, 2);
        assert_eq!(c.due(Some(vec![1]), true, 2), Some(2));
        assert_eq!(c.due(Some(vec![1]), true, 2), Some(2));
    }

    #[test]
    fn without_a_pid_list_every_pass_is_full() {
        let mut c = Cadence::new(10);
        c.due(None, false, 2);
        assert_eq!(c.due(None, false, 2), Some(2));
    }
}
