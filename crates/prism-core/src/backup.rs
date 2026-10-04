//! Export / import de toute la configuration de Prism dans un seul fichier, pour la
//! retrouver après une réinstallation ou sur un autre PC : barre (thème compris),
//! configuration de Prism, réglages d'apparence, niveaux d'allègement et de vie
//! privée appliqués. Les journaux ne sont jamais exportés : ils décrivent l'état
//! d'origine de *cette* machine.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::allege::{self, StartType, Tier};
use crate::apparence::{self, KnobValue};
use crate::bar::BarConfig;
use crate::privacy::{self, Level};

/// Version du format de fichier.
pub const FORMAT: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Backup {
    pub format: u32,
    /// Version de Prism qui a écrit le fichier (information).
    pub prism_version: String,
    pub bar: BarConfig,
    /// `config.toml` de l'utilisateur, s'il en avait un.
    #[serde(default)]
    pub config_toml: Option<String>,
    /// Réglages d'apparence : identifiant -> valeur.
    #[serde(default)]
    pub appearance: BTreeMap<String, KnobValue>,
    /// Niveaux d'allègement entièrement appliqués.
    #[serde(default)]
    pub allege: Vec<Tier>,
    /// Niveau de vie privée entièrement appliqué.
    #[serde(default)]
    pub privacy: Option<Level>,
}

/// Lecture d'un fichier exporté : format vérifié, configuration et réglages validés.
/// Renvoie aussi les avertissements (réglages inconnus de cette version, ignorés).
pub fn parse(bytes: &[u8]) -> Result<(Backup, Vec<String>), String> {
    let b = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    let mut backup: Backup = serde_json::from_slice(b).map_err(|e| format!("fichier d'export illisible : {e}"))?;
    if backup.format != FORMAT {
        return Err(format!(
            "format {} non pris en charge (cette version lit le format {FORMAT})",
            backup.format
        ));
    }
    if let Some(t) = &backup.config_toml {
        crate::config::Config::parse(t).map_err(|e| format!("configuration exportée invalide : {e}"))?;
    }
    let catalog = apparence::Catalog::builtin();
    let mut warnings = Vec::new();
    backup.appearance.retain(|id, v| match catalog.knob(id) {
        None => {
            warnings.push(format!("réglage d'apparence inconnu ignoré : {id}"));
            false
        }
        Some(k) if !k.options.iter().any(|o| &o.value == v) => {
            warnings.push(format!("valeur non proposée ignorée : {}", k.label));
            false
        }
        Some(_) => true,
    });
    backup.bar = backup.bar.sanitized();
    Ok((backup, warnings))
}

pub fn to_json(backup: &Backup) -> Result<String, String> {
    serde_json::to_string_pretty(backup).map_err(|e| e.to_string())
}

/// Niveaux d'allègement dont chaque élément est dans l'état voulu (ou absent).
pub fn allege_applied(sys: &mut dyn allege::SystemConfig, catalog: &allege::Catalog) -> Vec<Tier> {
    let mut out = Vec::new();
    for tier in [Tier::Sur, Tier::Avance, Tier::Jeu, Tier::Extreme] {
        let services = catalog.services.iter().filter(|s| s.tier == tier);
        let policies = catalog.policies.iter().filter(|p| p.tier == tier);
        let tasks = catalog.tasks.iter().filter(|t| t.tier == tier);
        let mut any = false;
        let mut all = true;
        for s in services {
            any = true;
            all &= match sys.service_start(&s.name) {
                Ok(None) => true,
                Ok(Some(st)) => st == s.start || (st == StartType::Disabled && s.start == StartType::Manual),
                Err(_) => false,
            };
        }
        for p in policies {
            any = true;
            all &= matches!(sys.policy(&p.full_key(), &p.value), Ok(Some(d)) if d == p.data);
        }
        for t in tasks {
            any = true;
            all &= matches!(sys.task_enabled(&t.path), Ok(None) | Ok(Some(false)));
        }
        for a in catalog.apps.iter().filter(|a| a.tier == tier) {
            any = true;
            all &= matches!(sys.app_installed(&a.package), Ok(false));
        }
        if any && all {
            out.push(tier);
        }
    }
    out
}

/// Niveau de vie privée entièrement en place (les composants absents ne comptent pas).
pub fn privacy_applied(rows: &[privacy::Row]) -> Option<Level> {
    let complete = |level: Level| {
        rows.iter()
            .filter(|r| r.level.is_some_and(|l| l <= level))
            .all(|r| matches!(r.state, privacy::State::On | privacy::State::Absent))
    };
    if complete(Level::Strict) {
        Some(Level::Strict)
    } else if complete(Level::Recommande) {
        Some(Level::Recommande)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Backup {
        let mut bar = BarConfig::default();
        bar.theme.preset = "dracula".into();
        bar.edge = crate::bar::Edge::Left;
        Backup {
            format: FORMAT,
            prism_version: "0.1.0".into(),
            bar,
            config_toml: None,
            appearance: BTreeMap::from([("anim_minmax".to_string(), KnobValue::Bool(false))]),
            allege: vec![Tier::Sur],
            privacy: Some(Level::Recommande),
        }
    }

    #[test]
    fn export_then_import_round_trips() {
        let b = sample();
        let json = to_json(&b).unwrap();
        let (back, warnings) = parse(json.as_bytes()).unwrap();
        assert_eq!(back, b);
        assert!(warnings.is_empty(), "{warnings:?}");
        // Marque UTF-8 (fichier passé par un éditeur Windows) acceptée.
        let mut bom = b"\xef\xbb\xbf".to_vec();
        bom.extend(json.as_bytes());
        assert!(parse(&bom).is_ok());
    }

    #[test]
    fn unknown_settings_are_dropped_with_a_warning_and_bad_files_refused() {
        let mut b = sample();
        b.appearance.insert("inexistant".into(), KnobValue::Bool(true));
        let (back, warnings) = parse(to_json(&b).unwrap().as_bytes()).unwrap();
        assert!(!back.appearance.contains_key("inexistant"));
        assert_eq!(warnings.len(), 1);
        let mut future = sample();
        future.format = 99;
        assert!(parse(to_json(&future).unwrap().as_bytes()).is_err());
        let mut bad_cfg = sample();
        bad_cfg.config_toml = Some("[[[ pas du toml".into());
        assert!(parse(to_json(&bad_cfg).unwrap().as_bytes()).is_err());
        assert!(parse(b"{\"format\":1}").is_err(), "barre manquante");
    }

    #[test]
    fn applied_levels_are_detected_from_the_system_state() {
        let catalog = allege::Catalog::builtin();
        let mut sys = allege::MockSystem::default();
        assert!(allege_applied(&mut sys, &catalog).is_empty() || !catalog.policies.is_empty());
        let changes = allege::plan(&catalog, &[Tier::Sur]);
        let mut j = allege::AllegeJournal::default();
        // La simulation n'a pas les services : on les crée en démarrage automatique.
        for s in &catalog.services {
            sys.services.insert(s.name.to_ascii_lowercase(), StartType::Auto);
        }
        for t in &catalog.tasks {
            sys.tasks.insert(t.path.to_ascii_lowercase(), true);
        }
        assert!(!allege_applied(&mut sys, &catalog).contains(&Tier::Sur));
        allege::apply(&mut sys, &catalog, &changes, &mut j, &mut |_| Ok(()));
        let applied = allege_applied(&mut sys, &catalog);
        assert!(
            applied.contains(&Tier::Sur) && !applied.contains(&Tier::Avance),
            "{applied:?}"
        );

        let pc = privacy::Catalog::builtin();
        let mut ps = privacy::MockPrivacy::default();
        assert_eq!(privacy_applied(&privacy::status(&mut ps, &pc)), None);
        let mut pj = privacy::Journal::default();
        privacy::apply(
            &mut ps,
            &privacy::plan(&pc, Level::Recommande),
            &mut pj,
            &mut |_| Ok(()),
        );
        assert_eq!(privacy_applied(&privacy::status(&mut ps, &pc)), Some(Level::Recommande));
        privacy::apply(&mut ps, &privacy::plan(&pc, Level::Strict), &mut pj, &mut |_| Ok(()));
        assert_eq!(privacy_applied(&privacy::status(&mut ps, &pc)), Some(Level::Strict));
    }
}
