//! Prism OS — cœur de décision.
//!
//! Ce crate décide *quoi* faire (classement des processus, plan du Mode Jeu, politique
//! RAM, journal de restauration, catalogue d'outils) sans jamais appeler Windows.
//! L'exécution passe par le trait [`platform::Platform`].

pub mod allege;
pub mod apparence;
pub mod classify;
pub mod config;
pub mod cores;
pub mod daily;
pub mod demarrage;
pub mod engine;
pub mod etat;
pub mod glob;
pub mod journal;
pub mod library;
pub mod mock;
pub mod model;
pub mod paths;
pub mod plan;
pub mod platform;
pub mod tools;
pub mod watch;
