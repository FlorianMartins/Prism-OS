//! Prism OS — cœur de décision.
//!
//! Ce crate décide *quoi* faire (classement des processus, plan du Mode Jeu, politique
//! RAM, journal de restauration, catalogue d'outils) sans jamais appeler Windows.
//! L'exécution passe par le trait [`platform::Platform`].

pub mod allege;
pub mod apparence;
pub mod backup;
pub mod bar;
pub mod cadence;
pub mod classify;
pub mod config;
pub mod cores;
pub mod daily;
pub mod demarrage;
pub mod engine;
pub mod etat;
pub mod fx;
pub mod fx_effects;
pub mod glob;
pub mod jeux;
pub mod journal;
pub mod library;
pub mod mock;
pub mod model;
pub mod noyau;
pub mod paths;
pub mod plan;
pub mod platform;
pub mod privacy;
pub mod rapport;
pub mod start_menu;
pub mod theme;
pub mod tiling;
pub mod tools;
pub mod update;
pub mod watch;
pub mod webview;
pub mod wobbly;
