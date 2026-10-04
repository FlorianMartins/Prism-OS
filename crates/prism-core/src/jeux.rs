//! Réglages par jeu (page Jeux) et bilan de la dernière partie.
//!
//! Les réglages sont ceux que Windows propose lui-même pour un exécutable — rien n'est
//! écrit dans le jeu ni injecté :
//! - carte graphique haute performance (Paramètres > Système > Écran > Graphiques :
//!   `HKCU\Software\Microsoft\DirectX\UserGpuPreferences`) ;
//! - optimisations plein écran désactivées (Propriétés > Compatibilité :
//!   `HKCU\…\AppCompatFlags\Layers`, drapeau `DISABLEDXMAXIMIZEDWINDOWEDMODE`).
//!
//! La valeur d'origine de chaque exécutable est journalisée : décocher la remet.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::allege::{RegData, SystemConfig};
use crate::glob::any_matches;

pub const GPU_KEY: &str = "HKCU\\Software\\Microsoft\\DirectX\\UserGpuPreferences";
pub const LAYERS_KEY: &str = "HKCU\\Software\\Microsoft\\Windows NT\\CurrentVersion\\AppCompatFlags\\Layers";
const GPU_HIGH: &str = "GpuPreference=2;";
const FSO_FLAG: &str = "DISABLEDXMAXIMIZEDWINDOWEDMODE";

/// Au plus autant d'exécutables par jeu (un jeu en a rarement plus de deux utiles).
const MAX_EXES: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reglage {
    /// Carte graphique haute performance.
    Gpu,
    /// Optimisations plein écran désactivées.
    PleinEcran,
}

impl Reglage {
    pub fn label(self) -> &'static str {
        match self {
            Reglage::Gpu => "Carte graphique haute performance",
            Reglage::PleinEcran => "Plein écran exclusif (optimisations plein écran désactivées)",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Reglage::Gpu => GPU_KEY,
            Reglage::PleinEcran => LAYERS_KEY,
        }
    }

    fn is_on(self, data: Option<&RegData>) -> bool {
        match (self, data) {
            (Reglage::Gpu, Some(RegData::Text(t))) => t.contains("GpuPreference=2"),
            (Reglage::PleinEcran, Some(RegData::Text(t))) => t.split_whitespace().any(|f| f == FSO_FLAG),
            _ => false,
        }
    }

    /// Valeur à écrire, en gardant ce qui y était déjà (autres drapeaux de compatibilité).
    fn on_value(self, current: Option<&RegData>) -> RegData {
        match (self, current) {
            (Reglage::Gpu, _) => RegData::Text(GPU_HIGH.into()),
            (Reglage::PleinEcran, Some(RegData::Text(t))) if !t.trim().is_empty() => {
                let rest = t.trim().trim_start_matches('~').trim();
                RegData::Text(format!("~ {rest} {FSO_FLAG}").replace("  ", " "))
            }
            (Reglage::PleinEcran, _) => RegData::Text(format!("~ {FSO_FLAG}")),
        }
    }
}

/// Exécutables probables d'un jeu, parmi les fichiers de son dossier (chemin relatif en
/// minuscules ou non, taille). Écartés : utilitaires connus (`game_helpers`),
/// désinstalleurs, redistribuables, rapporteurs de plantage. Les plus gros d'abord : le
/// jeu est presque toujours le plus gros exécutable.
pub fn candidate_exes(files: &[(String, u64)], helpers: &[String]) -> Vec<String> {
    const SKIP_DIRS: [&str; 6] = ["_commonredist", "redist", "directx", "vcredist", "support", "installer"];
    const SKIP_WORDS: [&str; 6] = ["unins", "crash", "redist", "setup", "report", "launcherpatcher"];
    let mut out: Vec<(String, u64)> = files
        .iter()
        .filter(|(p, _)| p.to_ascii_lowercase().ends_with(".exe"))
        .filter(|(p, _)| {
            let l = p.to_ascii_lowercase().replace('/', "\\");
            let name = l.rsplit('\\').next().unwrap_or(&l).to_string();
            !l.split('\\').any(|d| SKIP_DIRS.contains(&d))
                && !SKIP_WORDS.iter().any(|w| name.contains(w))
                && !any_matches(helpers, &name)
        })
        .cloned()
        .collect();
    out.sort_by_key(|(_, size)| std::cmp::Reverse(*size));
    out.truncate(MAX_EXES);
    out.into_iter().map(|(p, _)| p).collect()
}

/// Valeur d'origine d'un réglage pour un exécutable.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Origine {
    pub reglage: Reglage,
    pub exe: String,
    pub was: Option<RegData>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Journal {
    pub origines: Vec<Origine>,
}

impl Journal {
    pub const FICHIER: &'static str = "jeux-reglages.json";

    pub fn charger(dir: &Path) -> Journal {
        std::fs::read(dir.join(Self::FICHIER))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn enregistrer(&self, dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let json = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(dir.join(Self::FICHIER), json).map_err(|e| e.to_string())
    }
}

/// Le réglage est-il en place pour tous ces exécutables ? `None` : aucun exécutable.
pub fn etat(sys: &mut dyn SystemConfig, reglage: Reglage, exes: &[String]) -> Option<bool> {
    if exes.is_empty() {
        return None;
    }
    Some(
        exes.iter()
            .all(|e| reglage.is_on(sys.policy(reglage.key(), e).ok().flatten().as_ref())),
    )
}

/// Active ou retire le réglage pour ces exécutables. À l'activation, l'origine de
/// chacun est journalisée (une fois) ; au retrait, elle est remise.
pub fn appliquer(
    sys: &mut dyn SystemConfig,
    journal: &mut Journal,
    reglage: Reglage,
    exes: &[String],
    on: bool,
) -> Result<usize, String> {
    let mut n = 0;
    for exe in exes {
        let current = sys.policy(reglage.key(), exe)?;
        if on {
            if reglage.is_on(current.as_ref()) {
                continue;
            }
            if !journal
                .origines
                .iter()
                .any(|o| o.reglage == reglage && o.exe.eq_ignore_ascii_case(exe))
            {
                journal.origines.push(Origine {
                    reglage,
                    exe: exe.clone(),
                    was: current.clone(),
                });
            }
            sys.set_policy(reglage.key(), exe, &reglage.on_value(current.as_ref()))?;
            n += 1;
        } else {
            let pos = journal
                .origines
                .iter()
                .position(|o| o.reglage == reglage && o.exe.eq_ignore_ascii_case(exe));
            match pos.map(|i| journal.origines.remove(i)) {
                Some(Origine { was: Some(d), .. }) => sys.set_policy(reglage.key(), exe, &d)?,
                Some(Origine { was: None, .. }) => sys.delete_policy(reglage.key(), exe)?,
                // Réglé avant Prism (ou à la main) : on retire seulement ce que Prism pose.
                None if reglage.is_on(current.as_ref()) => sys.delete_policy(reglage.key(), exe)?,
                None => continue,
            }
            n += 1;
        }
    }
    Ok(n)
}

/// Remet toutes les origines (désinstallation de Prism).
pub fn tout_remettre(sys: &mut dyn SystemConfig, journal: &mut Journal) -> Vec<String> {
    let mut errors = Vec::new();
    for o in std::mem::take(&mut journal.origines) {
        let r = match &o.was {
            Some(d) => sys.set_policy(o.reglage.key(), &o.exe, d),
            None => sys.delete_policy(o.reglage.key(), &o.exe),
        };
        if let Err(e) = r {
            errors.push(format!("{} : {e}", o.exe));
            journal.origines.push(o);
        }
    }
    errors
}

/// Bilan de la dernière partie, écrit par le moteur à la fin du Mode Jeu.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Partie {
    pub jeux: Vec<String>,
    /// Début, heure UTC lisible.
    pub debut: String,
    pub duree_secs: u64,
    /// Réglages du Mode Jeu appliqués au début (applis en retrait, services en pause…).
    pub actions: usize,
    /// Mémoire disponible au début de la partie et au plus bas pendant, en octets.
    pub dispo_debut: u64,
    pub dispo_min: u64,
    /// Anti-cheat noyau détecté (plan « jeu noyau » joué), s'il y en a eu un.
    pub anticheat: Option<String>,
}

impl Partie {
    pub const FICHIER: &'static str = "derniere-partie.json";

    pub fn charger(dir: &Path) -> Option<Partie> {
        std::fs::read(dir.join(Self::FICHIER))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
    }

    pub fn enregistrer(&self, dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let json = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(dir.join(Self::FICHIER), json).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::allege::MockSystem;

    fn helpers() -> Vec<String> {
        crate::config::Config::builtin().lists.game_helpers
    }

    #[test]
    fn the_game_exe_is_found_and_helpers_are_skipped() {
        let files = vec![
            (r"Binaries\Win64\Game-Win64-Shipping.exe".to_string(), 180 << 20),
            (r"Game.exe".to_string(), 1 << 20),
            (r"Engine\Binaries\Win64\CrashReportClient.exe".to_string(), 20 << 20),
            (r"_CommonRedist\vcredist\2019\VC_redist.x64.exe".to_string(), 25 << 20),
            (r"unins000.exe".to_string(), 3 << 20),
            (r"readme.txt".to_string(), 1),
        ];
        let exes = candidate_exes(&files, &helpers());
        assert_eq!(exes, [r"Binaries\Win64\Game-Win64-Shipping.exe", "Game.exe"]);
    }

    #[test]
    fn settings_are_applied_then_the_exact_original_comes_back() {
        let mut m = MockSystem::default();
        let mut j = Journal::default();
        let exe = r"c:\games\x\game.exe".to_string();
        // Un drapeau de compatibilité déjà posé par l'utilisateur.
        m.policies.insert(
            (LAYERS_KEY.to_ascii_lowercase(), exe.clone()),
            RegData::Text("~ HIGHDPIAWARE".into()),
        );
        let exes = vec![exe.clone()];
        assert_eq!(etat(&mut m, Reglage::PleinEcran, &exes), Some(false));
        appliquer(&mut m, &mut j, Reglage::PleinEcran, &exes, true).unwrap();
        appliquer(&mut m, &mut j, Reglage::Gpu, &exes, true).unwrap();
        assert_eq!(etat(&mut m, Reglage::PleinEcran, &exes), Some(true));
        assert_eq!(etat(&mut m, Reglage::Gpu, &exes), Some(true));
        let layers = &m.policies[&(LAYERS_KEY.to_ascii_lowercase(), exe.clone())];
        assert_eq!(
            *layers,
            RegData::Text("~ HIGHDPIAWARE DISABLEDXMAXIMIZEDWINDOWEDMODE".into())
        );
        appliquer(&mut m, &mut j, Reglage::PleinEcran, &exes, false).unwrap();
        appliquer(&mut m, &mut j, Reglage::Gpu, &exes, false).unwrap();
        assert_eq!(
            m.policies[&(LAYERS_KEY.to_ascii_lowercase(), exe.clone())],
            RegData::Text("~ HIGHDPIAWARE".into())
        );
        assert!(!m.policies.contains_key(&(GPU_KEY.to_ascii_lowercase(), exe)));
        assert!(j.origines.is_empty());
        assert_eq!(etat(&mut m, Reglage::Gpu, &[]), None);
    }

    #[test]
    fn both_registry_keys_are_allowed_and_nothing_else_is_opened() {
        assert!(crate::allege::registry_allowed(GPU_KEY));
        assert!(crate::allege::registry_allowed(LAYERS_KEY));
        assert!(!crate::allege::registry_allowed(
            "HKCU\\Software\\Microsoft\\Windows NT\\CurrentVersion\\AppCompatFlags\\Compatibility Assistant"
        ));
    }
}
