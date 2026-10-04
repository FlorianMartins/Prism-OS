//! Interface graphique de Prism OS (egui).
//!
//! L'interface ne parle qu'au trait [`backend::Backend`] : Windows en vrai,
//! [`mock::MockBackend`] pour les tests et les captures d'écran.

pub mod app;
pub mod appearance_common;
pub mod backend;
pub mod mock;
pub mod theme;
#[cfg(windows)]
pub mod winbackend;
