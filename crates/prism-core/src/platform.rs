//! Frontière entre la décision (le cœur) et l'exécution (Windows, ou une simulation).

use crate::journal::Undo;
use crate::model::Snapshot;
use crate::plan::Action;

/// Résultat d'une action ou d'une annulation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Fait. `Some(undo)` si l'action est réversible : c'est l'état d'origine.
    Done(Option<Undo>),
    /// Rien fait, pour une raison normale (processus disparu, droits insuffisants…).
    Skipped(String),
    /// Erreur inattendue.
    Failed(String),
}

pub trait Platform {
    fn snapshot(&mut self) -> Result<Snapshot, String>;

    /// Applique l'action. Une action qui vise un processus doit vérifier son
    /// identité complète (PID + date de création) avant d'agir, et ne jamais
    /// *remonter* une priorité (CPU ou mémoire) déjà plus basse que la cible :
    /// elle répond alors `Skipped(mock::ALREADY_LOWER)` et rien n'est journalisé.
    fn apply(&mut self, action: &Action) -> Outcome;

    /// Remet l'état d'origine, avec la même vérification d'identité.
    fn undo(&mut self, undo: &Undo) -> Outcome;
}
