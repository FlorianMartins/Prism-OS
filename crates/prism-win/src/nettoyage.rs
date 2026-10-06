//! Nettoyage de la RAM au clic, avec un résultat immédiat : les services désactivés
//! ou mis à la demande par Prism qui tournent encore sont arrêtés (ils gardaient leur
//! mémoire jusqu'au redémarrage), puis la mémoire des applis en arrière-plan est rendue
//! et le cache basse priorité purgé.

use prism_core::allege::{services_to_stop, AllegeJournal, Catalog};
use prism_core::config::Config;
use prism_core::engine::Report;
use prism_core::model::MemStatus;
use prism_core::platform::{Outcome, Platform};

use crate::WindowsPlatform;

pub struct Nettoyage {
    pub before: MemStatus,
    pub after: MemStatus,
    /// Services arrêtés.
    pub services: Vec<String>,
    pub report: Report,
}

impl Nettoyage {
    /// « Mémoire utilisée : 9,9 → 7,4 Go (−2,5 Go) · 3 services arrêtés ».
    pub fn resume(&self) -> String {
        prism_core::engine::clean_summary(self.before.used(), self.after.used(), self.services.len())
    }
}

pub fn nettoyer(cfg: &Config, deep: bool) -> Result<Nettoyage, String> {
    let mut w = WindowsPlatform::new();
    let before = w.snapshot()?.mem;
    let journal = AllegeJournal::load(&prism_core::paths::data_dir().join("allegement.json")).unwrap_or_default();
    let names = crate::services::services_list()
        .map(|l| services_to_stop(&Catalog::builtin(), &journal, &l))
        .unwrap_or_default();
    let services: Vec<String> = names
        .into_iter()
        .filter(|n| matches!(crate::sysconfig::pause_service(n), Outcome::Done(_)))
        .collect();
    if !services.is_empty() {
        // Le temps que les processus des services se ferment et rendent leur mémoire.
        for _ in 0..25 {
            if services.iter().all(|n| crate::sysconfig::service_pid(n).is_none()) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
    }
    let report = prism_core::engine::clean(&mut w, cfg, deep)?;
    // La mémoire rendue passe d'abord par la file « modifiée » (comptée comme utilisée)
    // avant d'être écrite puis libérée : mesurée tout de suite, le gain était sous-estimé.
    std::thread::sleep(std::time::Duration::from_secs(2));
    let after = w.snapshot()?.mem;
    Ok(Nettoyage {
        before,
        after,
        services,
        report,
    })
}
