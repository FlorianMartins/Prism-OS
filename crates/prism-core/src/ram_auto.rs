//! Nettoyage automatique de la RAM : la même action que le bouton « Libérer la RAM des
//! applis inactives », lancée par le moteur à intervalle régulier et dès que la mémoire
//! utilisée dépasse un seuil. Réglable par l'utilisateur, actif par défaut.

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Reglages {
    pub actif: bool,
    /// Un nettoyage toutes les N minutes (5 à 120).
    pub toutes_les_minutes: u64,
    /// Et dès que la mémoire utilisée dépasse ce pourcentage (50 à 95 ; 0 : jamais).
    pub seuil_pourcent: u64,
}

impl Default for Reglages {
    fn default() -> Self {
        Reglages {
            actif: true,
            toutes_les_minutes: 15,
            seuil_pourcent: 75,
        }
    }
}

/// Deux nettoyages sont espacés d'au moins 2 minutes (une appli qui reprend sa mémoire
/// tout de suite ne doit pas déclencher un nettoyage à chaque passe).
pub const MIN_ECART_SECS: u64 = 120;

impl Reglages {
    pub const FICHIER: &'static str = "ram-auto.json";

    pub fn charger(dir: &Path) -> Reglages {
        std::fs::read(dir.join(Self::FICHIER))
            .ok()
            .and_then(|b| serde_json::from_slice::<Reglages>(&b).ok())
            .unwrap_or_default()
            .borne()
    }

    pub fn enregistrer(&self, dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let json = serde_json::to_vec_pretty(&self.clone().borne()).map_err(|e| e.to_string())?;
        std::fs::write(dir.join(Self::FICHIER), json).map_err(|e| e.to_string())
    }

    /// Valeurs ramenées dans leurs bornes (fichier modifié à la main).
    pub fn borne(mut self) -> Reglages {
        self.toutes_les_minutes = self.toutes_les_minutes.clamp(5, 120);
        if self.seuil_pourcent != 0 {
            self.seuil_pourcent = self.seuil_pourcent.clamp(50, 95);
        }
        self
    }

    /// Faut-il nettoyer maintenant ? `depuis_secs` : temps depuis le dernier nettoyage
    /// (ou depuis le démarrage du moteur) ; `utilisee_pourcent` : mémoire utilisée.
    pub fn du(&self, depuis_secs: u64, utilisee_pourcent: u64) -> bool {
        if !self.actif || depuis_secs < MIN_ECART_SECS {
            return false;
        }
        depuis_secs >= self.toutes_les_minutes * 60
            || (self.seuil_pourcent > 0 && utilisee_pourcent >= self.seuil_pourcent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_on_every_15_min_or_above_75_percent() {
        let r = Reglages::default();
        assert!(!r.du(14 * 60, 40));
        assert!(r.du(15 * 60, 40));
        assert!(r.du(3 * 60, 80), "seuil dépassé");
        assert!(!r.du(60, 99), "au moins 2 minutes entre deux nettoyages");
    }

    #[test]
    fn off_or_threshold_disabled() {
        let off = Reglages {
            actif: false,
            ..Default::default()
        };
        assert!(!off.du(10_000, 99));
        let no_threshold = Reglages {
            seuil_pourcent: 0,
            ..Default::default()
        };
        assert!(!no_threshold.du(5 * 60, 99));
    }

    #[test]
    fn values_are_kept_in_bounds() {
        let r = Reglages {
            actif: true,
            toutes_les_minutes: 1,
            seuil_pourcent: 10,
        }
        .borne();
        assert_eq!((r.toutes_les_minutes, r.seuil_pourcent), (5, 50));
    }
}
