//! Vie privée (v0.5) : stratégies officielles, réglages documentés de Paramètres et
//! règles du Pare-feu Windows qui coupent les composants de télémétrie d'Internet.
//! Tout est journalisé et réversible ; le tableau de bord lit aussi, sans y toucher,
//! ce que l'allègement fait déjà.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::allege::{Hive, RegData, StartType};

pub const VIE_PRIVEE_TOML: &str = include_str!("../../../config/vie-privee.toml");

/// Préfixe des règles de pare-feu créées par Prism (retrouvées et supprimées par lui).
pub const RULE_PREFIX: &str = "Prism OS - ";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    /// Aucune fonction utile perdue.
    Recommande,
    /// On renonce à quelque chose (écrit dans `lose`).
    Strict,
}

impl Level {
    pub fn parse(s: &str) -> Option<Level> {
        match s.to_lowercase().replace('é', "e").as_str() {
            "recommande" | "recommended" => Some(Level::Recommande),
            "strict" => Some(Level::Strict),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Level::Recommande => "Recommandé",
            Level::Strict => "Strict",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Telemetrie,
    Publicite,
    Recherche,
    Saisie,
    Ia,
    Localisation,
    Navigateur,
}

impl Category {
    pub const ALL: [Category; 7] = [
        Category::Telemetrie,
        Category::Publicite,
        Category::Recherche,
        Category::Saisie,
        Category::Ia,
        Category::Localisation,
        Category::Navigateur,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Category::Telemetrie => "Télémétrie",
            Category::Publicite => "Publicité et suggestions",
            Category::Recherche => "Recherche",
            Category::Saisie => "Saisie et voix",
            Category::Ia => "IA",
            Category::Localisation => "Localisation et appareils",
            Category::Navigateur => "Navigateur (Edge)",
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegItem {
    #[serde(default)]
    pub hive: Hive,
    pub key: String,
    pub value: String,
    pub data: RegData,
    #[serde(default)]
    pub reboot: bool,
    pub label: String,
    pub category: Category,
    pub level: Level,
    pub why: String,
    #[serde(default)]
    pub lose: Option<String>,
}

impl RegItem {
    pub fn full_key(&self) -> String {
        format!("{}\\{}", self.hive.prefix(), self.key)
    }
}

/// Cible d'une règle de pare-feu.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FwTarget {
    Service(String),
    /// Chemin avec `%SystemRoot%`, comme l'accepte le Pare-feu Windows.
    Program(String),
}

impl FwTarget {
    pub fn describe(&self) -> String {
        match self {
            FwTarget::Service(s) => format!("service {s}"),
            FwTarget::Program(p) => p.clone(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FirewallItem {
    pub id: String,
    #[serde(default)]
    pub service: Option<String>,
    #[serde(default)]
    pub program: Option<String>,
    pub label: String,
    pub category: Category,
    pub level: Level,
    pub why: String,
    #[serde(default)]
    pub lose: Option<String>,
}

impl FirewallItem {
    pub fn rule_name(&self) -> String {
        format!("{RULE_PREFIX}{}", self.id)
    }

    pub fn target(&self) -> Option<FwTarget> {
        match (&self.service, &self.program) {
            (Some(s), None) => Some(FwTarget::Service(s.clone())),
            (None, Some(p)) => Some(FwTarget::Program(p.clone())),
            _ => None,
        }
    }
}

/// Lecture seule : ce que fait déjà un autre module de Prism.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckItem {
    /// Service attendu désactivé.
    #[serde(default)]
    pub service: Option<String>,
    /// Valeur de registre attendue (chemin complet, ruche comprise).
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub data: Option<RegData>,
    /// Tâche planifiée attendue désactivée.
    #[serde(default)]
    pub task: Option<String>,
    pub label: String,
    pub category: Category,
    pub source: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    #[serde(default)]
    pub registry: Vec<RegItem>,
    #[serde(default)]
    pub firewall: Vec<FirewallItem>,
    #[serde(default)]
    pub check: Vec<CheckItem>,
}

/// Clés que la vie privée peut écrire : les stratégies officielles, et exactement ces
/// quelques clés de réglages de Paramètres (pas leurs sous-clés).
pub const PRIVACY_POLICY_PREFIXES: [&str; 2] = ["hklm\\software\\policies\\", "hkcu\\software\\policies\\"];
pub const PRIVACY_EXACT_KEYS: [&str; 4] = [
    "hkcu\\software\\microsoft\\windows\\currentversion\\contentdeliverymanager",
    "hkcu\\software\\microsoft\\windows\\currentversion\\explorer\\advanced",
    "hkcu\\software\\microsoft\\inputpersonalization",
    "hkcu\\control panel\\international\\user profile",
];

pub fn registry_allowed(full_key: &str) -> bool {
    let k = full_key.to_ascii_lowercase();
    PRIVACY_POLICY_PREFIXES.iter().any(|p| k.starts_with(p)) || PRIVACY_EXACT_KEYS.contains(&k.as_str())
}

/// Un programme bloqué doit être un exécutable de Windows lui-même.
pub fn program_allowed(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    p.starts_with("%systemroot%\\") && p.ends_with(".exe") && !p.contains("..") && !p.contains('/')
}

impl Catalog {
    pub fn parse(text: &str) -> Result<Catalog, String> {
        let c: Catalog = toml::from_str(text).map_err(|e| format!("catalogue vie privée illisible : {e}"))?;
        c.validate()?;
        Ok(c)
    }

    pub fn builtin() -> Catalog {
        Catalog::parse(VIE_PRIVEE_TOML).expect("catalogue vie privée embarqué valide")
    }

    pub fn validate(&self) -> Result<(), String> {
        let protected = crate::allege::Catalog::builtin();
        for r in &self.registry {
            if !registry_allowed(&r.full_key()) {
                return Err(format!("{} : clé hors de la liste autorisée", r.full_key()));
            }
            if r.level == Level::Strict && r.lose.is_none() {
                return Err(format!("{} : un réglage strict doit dire ce qu'on perd", r.label));
            }
        }
        let mut ids = std::collections::HashSet::new();
        for f in &self.firewall {
            if f.id.is_empty()
                || !f
                    .id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            {
                return Err(format!("{} : identifiant de règle invalide", f.id));
            }
            if !ids.insert(f.id.clone()) {
                return Err(format!("{} : identifiant de règle en double", f.id));
            }
            match f.target() {
                None => return Err(format!("{} : une règle vise un service OU un programme", f.id)),
                Some(FwTarget::Service(s)) => {
                    if let Some(reason) = protected.protection(&s) {
                        return Err(format!("{} : service protégé ({reason})", f.id));
                    }
                }
                Some(FwTarget::Program(p)) => {
                    if !program_allowed(&p) {
                        return Err(format!(
                            "{} : seul un exécutable de Windows (%SystemRoot%) peut être bloqué",
                            f.id
                        ));
                    }
                }
            }
            if f.level == Level::Strict && f.lose.is_none() {
                return Err(format!("{} : une règle stricte doit dire ce qu'on perd", f.id));
            }
        }
        for c in &self.check {
            let kinds = c.service.is_some() as u8 + c.key.is_some() as u8 + c.task.is_some() as u8;
            if kinds != 1 || (c.key.is_some() && (c.value.is_none() || c.data.is_none())) {
                return Err(format!(
                    "{} : un contrôle vise un service, une valeur OU une tâche",
                    c.label
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "do", rename_all = "snake_case")]
pub enum Change {
    Reg { key: String, value: String, data: RegData },
    Rule { name: String, target: FwTarget },
}

impl Change {
    pub fn describe(&self) -> String {
        match self {
            Change::Reg { key, value, data } => format!("{key}\\{value} = {data}"),
            Change::Rule { name, target } => format!("pare-feu « {name} » : {} bloqué", target.describe()),
        }
    }

    fn key(&self) -> String {
        match self {
            Change::Reg { key, value, .. } => {
                format!("reg:{}\\{}", key.to_ascii_lowercase(), value.to_ascii_lowercase())
            }
            Change::Rule { name, .. } => format!("rule:{}", name.to_ascii_lowercase()),
        }
    }
}

/// Ce qu'il faut pour revenir en arrière.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "undo", rename_all = "snake_case")]
pub enum Original {
    /// `was: None` : la valeur n'existait pas, elle sera supprimée.
    Reg {
        key: String,
        value: String,
        was: Option<RegData>,
    },
    /// Règle créée par Prism : elle sera supprimée.
    Rule { name: String },
}

impl Original {
    fn key(&self) -> String {
        match self {
            Original::Reg { key, value, .. } => {
                format!("reg:{}\\{}", key.to_ascii_lowercase(), value.to_ascii_lowercase())
            }
            Original::Rule { name } => format!("rule:{}", name.to_ascii_lowercase()),
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Original::Reg {
                key,
                value,
                was: Some(d),
            } => format!("{key}\\{value} <- {d}"),
            Original::Reg { key, value, was: None } => format!("{key}\\{value} supprimée"),
            Original::Rule { name } => format!("règle de pare-feu « {name} » supprimée"),
        }
    }
}

/// Lecture et écriture de la configuration (Windows, ou simulation).
pub trait PrivacySystem {
    fn reg(&mut self, key: &str, value: &str) -> Result<Option<RegData>, String>;
    fn set_reg(&mut self, key: &str, value: &str, data: &RegData) -> Result<(), String>;
    fn delete_reg(&mut self, key: &str, value: &str) -> Result<(), String>;
    fn rule_exists(&mut self, name: &str) -> Result<bool, String>;
    fn add_rule(&mut self, name: &str, target: &FwTarget) -> Result<(), String>;
    fn delete_rule(&mut self, name: &str) -> Result<(), String>;
    /// Le programme ou le service visé existe-t-il sur cette machine ?
    fn target_exists(&mut self, target: &FwTarget) -> bool;
    /// `Ok(None)` : service absent.
    fn service_start(&mut self, name: &str) -> Result<Option<StartType>, String>;
    /// `Ok(None)` : tâche absente.
    fn task_enabled(&mut self, path: &str) -> Result<Option<bool>, String>;
}

/// Changements d'un niveau : « strict » comprend « recommandé ».
pub fn plan(catalog: &Catalog, level: Level) -> Vec<Change> {
    let regs = catalog
        .registry
        .iter()
        .filter(|r| r.level <= level)
        .map(|r| Change::Reg {
            key: r.full_key(),
            value: r.value.clone(),
            data: r.data.clone(),
        });
    let rules = catalog.firewall.iter().filter(|f| f.level <= level).filter_map(|f| {
        f.target().map(|target| Change::Rule {
            name: f.rule_name(),
            target,
        })
    });
    regs.chain(rules).collect()
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Journal {
    pub originals: Vec<Original>,
}

pub fn journal_path() -> PathBuf {
    crate::paths::data_dir().join("vie-privee.json")
}

impl Journal {
    pub fn load(path: &PathBuf) -> Result<Journal, String> {
        match fs::read(path) {
            Ok(b) => {
                serde_json::from_slice(&b).map_err(|e| format!("journal vie privée illisible {} : {e}", path.display()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Journal::default()),
            Err(e) => Err(format!("{} : {e}", path.display())),
        }
    }

    pub fn save(&self, path: &PathBuf) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| format!("{} : {e}", dir.display()))?;
        }
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        fs::rename(&tmp, path).map_err(|e| e.to_string())
    }

    fn has(&self, key: &str) -> bool {
        self.originals.iter().any(|o| o.key() == key)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub done: Vec<String>,
    /// Déjà fait, ou absent de cette machine.
    pub unchanged: Vec<String>,
    pub failed: Vec<String>,
}

/// Applique les changements ; chaque état d'origine est journalisé une seule fois et
/// le journal est sauvegardé après chaque changement.
pub fn apply(
    sys: &mut dyn PrivacySystem,
    changes: &[Change],
    journal: &mut Journal,
    save: &mut dyn FnMut(&Journal) -> Result<(), String>,
) -> Report {
    let mut r = Report::default();
    for c in changes {
        let what = c.describe();
        let original = match c {
            Change::Reg { key, value, data } => {
                // Dernière barrière, même avec un catalogue modifié à la main.
                if !registry_allowed(key) {
                    r.failed
                        .push(format!("{what} — refusé, clé hors de la liste autorisée"));
                    continue;
                }
                match sys.reg(key, value) {
                    Err(e) => Err(e),
                    Ok(Some(d)) if d == *data => {
                        r.unchanged.push(format!("{what} — déjà fait"));
                        continue;
                    }
                    Ok(was) => sys.set_reg(key, value, data).map(|()| Original::Reg {
                        key: key.clone(),
                        value: value.clone(),
                        was,
                    }),
                }
            }
            Change::Rule { name, target } => {
                if let FwTarget::Program(p) = target {
                    if !program_allowed(p) {
                        r.failed.push(format!("{what} — refusé, programme hors de Windows"));
                        continue;
                    }
                }
                if !sys.target_exists(target) {
                    r.unchanged.push(format!("{what} — absent de cette machine"));
                    continue;
                }
                match sys.rule_exists(name) {
                    Err(e) => Err(e),
                    Ok(true) => {
                        r.unchanged.push(format!("{what} — déjà fait"));
                        continue;
                    }
                    Ok(false) => sys
                        .add_rule(name, target)
                        .map(|()| Original::Rule { name: name.clone() }),
                }
            }
        };
        match original {
            Ok(o) => {
                if !journal.has(&c.key()) {
                    journal.originals.push(o);
                    if let Err(e) = save(journal) {
                        r.failed.push(format!("journal non écrit : {e}"));
                    }
                }
                r.done.push(what);
            }
            Err(e) => r.failed.push(format!("{what} — {e}")),
        }
    }
    r
}

/// Remet tout comme avant, du plus récent au plus ancien ; le journal ne garde que
/// ce qui n'a pas pu être remis.
pub fn restore(sys: &mut dyn PrivacySystem, journal: &mut Journal) -> Report {
    let mut r = Report::default();
    let mut kept = Vec::new();
    for o in journal.originals.iter().rev() {
        let res = match o {
            Original::Reg {
                key,
                value,
                was: Some(d),
            } => sys.set_reg(key, value, d),
            Original::Reg { key, value, was: None } => sys.delete_reg(key, value),
            Original::Rule { name } => {
                if name.starts_with(RULE_PREFIX) {
                    sys.delete_rule(name)
                } else {
                    Err("règle qui n'est pas de Prism : non supprimée".into())
                }
            }
        };
        match res {
            Ok(()) => r.done.push(o.describe()),
            Err(e) => {
                r.failed.push(format!("{} — {e}", o.describe()));
                kept.push(o.clone());
            }
        }
    }
    kept.reverse();
    journal.originals = kept;
    r
}

// --- Tableau de bord ----------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    /// Protection en place.
    On,
    Off,
    /// Le composant n'existe pas sur cette machine : rien à protéger.
    Absent,
    Unknown(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Setting,
    Firewall,
    /// Fait par un autre module (lecture seule).
    Check,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub label: String,
    pub category: Category,
    /// `None` : contrôle en lecture seule.
    pub level: Option<Level>,
    pub kind: Kind,
    pub state: State,
    pub why: String,
    pub lose: Option<String>,
    /// « Prism » ou le module qui s'en charge.
    pub source: String,
    pub reboot: bool,
}

pub fn status(sys: &mut dyn PrivacySystem, catalog: &Catalog) -> Vec<Row> {
    let mut rows = Vec::new();
    for r in &catalog.registry {
        let state = match sys.reg(&r.full_key(), &r.value) {
            Ok(Some(d)) if d == r.data => State::On,
            Ok(_) => State::Off,
            Err(e) => State::Unknown(e),
        };
        rows.push(Row {
            label: r.label.clone(),
            category: r.category,
            level: Some(r.level),
            kind: Kind::Setting,
            state,
            why: r.why.clone(),
            lose: r.lose.clone(),
            source: "Prism".into(),
            reboot: r.reboot,
        });
    }
    for f in &catalog.firewall {
        let state = match f.target() {
            Some(t) if !sys.target_exists(&t) => State::Absent,
            Some(_) => match sys.rule_exists(&f.rule_name()) {
                Ok(true) => State::On,
                Ok(false) => State::Off,
                Err(e) => State::Unknown(e),
            },
            None => State::Unknown("règle invalide".into()),
        };
        rows.push(Row {
            label: f.label.clone(),
            category: f.category,
            level: Some(f.level),
            kind: Kind::Firewall,
            state,
            why: f.why.clone(),
            lose: f.lose.clone(),
            source: "Prism".into(),
            reboot: false,
        });
    }
    for c in &catalog.check {
        let state = if let Some(s) = &c.service {
            match sys.service_start(s) {
                Ok(None) => State::Absent,
                Ok(Some(StartType::Disabled)) => State::On,
                Ok(Some(_)) => State::Off,
                Err(e) => State::Unknown(e),
            }
        } else if let (Some(k), Some(v), Some(d)) = (&c.key, &c.value, &c.data) {
            match sys.reg(k, v) {
                Ok(Some(x)) if x == *d => State::On,
                Ok(_) => State::Off,
                Err(e) => State::Unknown(e),
            }
        } else if let Some(t) = &c.task {
            match sys.task_enabled(t) {
                Ok(None) => State::Absent,
                Ok(Some(false)) => State::On,
                Ok(Some(true)) => State::Off,
                Err(e) => State::Unknown(e),
            }
        } else {
            State::Unknown("contrôle invalide".into())
        };
        rows.push(Row {
            label: c.label.clone(),
            category: c.category,
            level: None,
            kind: Kind::Check,
            state,
            why: String::new(),
            lose: None,
            source: c.source.clone(),
            reboot: false,
        });
    }
    rows
}

/// (protections en place, protections applicables) : les composants absents ne
/// comptent pas.
pub fn score(rows: &[Row]) -> (usize, usize) {
    let applicable = rows.iter().filter(|r| r.state != State::Absent).count();
    let on = rows.iter().filter(|r| r.state == State::On).count();
    (on, applicable)
}

/// Une connexion réseau ouverte par un composant de télémétrie (tableau de bord).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelemetryConnection {
    /// Composant (service ou programme).
    pub component: String,
    pub remote: String,
    pub state: String,
}

/// Simulation pour les tests et l'interface hors Windows.
#[derive(Clone, Debug, Default)]
pub struct MockPrivacy {
    pub reg: std::collections::BTreeMap<(String, String), RegData>,
    pub rules: std::collections::BTreeMap<String, FwTarget>,
    /// Cibles absentes de la machine simulée.
    pub missing: Vec<FwTarget>,
    pub services: std::collections::BTreeMap<String, StartType>,
    pub tasks: std::collections::BTreeMap<String, bool>,
}

impl PrivacySystem for MockPrivacy {
    fn reg(&mut self, key: &str, value: &str) -> Result<Option<RegData>, String> {
        Ok(self
            .reg
            .get(&(key.to_ascii_lowercase(), value.to_ascii_lowercase()))
            .cloned())
    }
    fn set_reg(&mut self, key: &str, value: &str, data: &RegData) -> Result<(), String> {
        self.reg
            .insert((key.to_ascii_lowercase(), value.to_ascii_lowercase()), data.clone());
        Ok(())
    }
    fn delete_reg(&mut self, key: &str, value: &str) -> Result<(), String> {
        self.reg.remove(&(key.to_ascii_lowercase(), value.to_ascii_lowercase()));
        Ok(())
    }
    fn rule_exists(&mut self, name: &str) -> Result<bool, String> {
        Ok(self.rules.contains_key(name))
    }
    fn add_rule(&mut self, name: &str, target: &FwTarget) -> Result<(), String> {
        self.rules.insert(name.to_string(), target.clone());
        Ok(())
    }
    fn delete_rule(&mut self, name: &str) -> Result<(), String> {
        self.rules.remove(name);
        Ok(())
    }
    fn target_exists(&mut self, target: &FwTarget) -> bool {
        !self.missing.contains(target)
    }
    fn service_start(&mut self, name: &str) -> Result<Option<StartType>, String> {
        Ok(self.services.get(name).copied())
    }
    fn task_enabled(&mut self, path: &str) -> Result<Option<bool>, String> {
        Ok(self.tasks.get(&path.to_ascii_lowercase()).copied())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(sys: &mut MockPrivacy, level: Level, journal: &mut Journal) -> Report {
        let c = Catalog::builtin();
        apply(sys, &plan(&c, level), journal, &mut |_| Ok(()))
    }

    #[test]
    fn builtin_catalog_is_valid_and_complete() {
        let c = Catalog::builtin();
        assert!(c.registry.len() >= 15, "{}", c.registry.len());
        assert!(c.firewall.len() >= 3);
        assert!(c.check.len() >= 5);
        for f in &c.firewall {
            assert!(f.rule_name().starts_with(RULE_PREFIX));
        }
    }

    #[test]
    fn recommended_is_a_subset_of_strict_and_strict_says_what_you_lose() {
        let c = Catalog::builtin();
        let rec = plan(&c, Level::Recommande);
        let strict = plan(&c, Level::Strict);
        assert!(rec.len() < strict.len());
        assert!(rec.iter().all(|x| strict.contains(x)));
        assert!(c
            .registry
            .iter()
            .all(|r| r.level == Level::Recommande || r.lose.is_some()));
    }

    #[test]
    fn apply_then_restore_gives_back_the_exact_original_state() {
        let mut sys = MockPrivacy::default();
        // État d'origine : une valeur déjà présente avec une autre donnée.
        sys.set_reg(
            "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Advanced",
            "Start_TrackProgs",
            &RegData::Dword(1),
        )
        .unwrap();
        let before = sys.reg.clone();
        let mut j = Journal::default();
        let r = run(&mut sys, Level::Strict, &mut j);
        assert!(r.failed.is_empty(), "{:?}", r.failed);
        assert!(!sys.rules.is_empty());
        let rows = status(&mut sys, &Catalog::builtin());
        assert!(rows
            .iter()
            .filter(|r| r.kind != Kind::Check)
            .all(|r| r.state == State::On));
        // Réappliquer ne change rien et ne double pas le journal.
        let n = j.originals.len();
        let again = run(&mut sys, Level::Strict, &mut j);
        assert!(again.done.is_empty() && j.originals.len() == n);
        let back = restore(&mut sys, &mut j);
        assert!(back.failed.is_empty());
        assert_eq!(sys.reg, before, "registre remis exactement");
        assert!(sys.rules.is_empty(), "règles de Prism supprimées");
        assert!(j.originals.is_empty());
    }

    #[test]
    fn a_rule_that_already_existed_is_not_deleted_on_restore() {
        let mut sys = MockPrivacy::default();
        let c = Catalog::builtin();
        let first = &c.firewall[0];
        sys.add_rule(&first.rule_name(), &first.target().unwrap()).unwrap();
        let mut j = Journal::default();
        run(&mut sys, Level::Recommande, &mut j);
        restore(&mut sys, &mut j);
        assert!(
            sys.rules.contains_key(&first.rule_name()),
            "règle d'avant Prism conservée"
        );
    }

    #[test]
    fn missing_components_are_skipped_and_do_not_count() {
        let mut sys = MockPrivacy::default();
        let c = Catalog::builtin();
        let t = c.firewall[1].target().unwrap();
        sys.missing.push(t);
        let mut j = Journal::default();
        let r = run(&mut sys, Level::Recommande, &mut j);
        assert!(r.unchanged.iter().any(|u| u.contains("absent")));
        let rows = status(&mut sys, &c);
        let (on, applicable) = score(&rows);
        assert_eq!(
            applicable,
            rows.len()
                - 1
                - rows
                    .iter()
                    .filter(|r| r.kind == Kind::Check && r.state == State::Absent)
                    .count()
        );
        assert!(
            on < applicable,
            "les contrôles d'allègement ne sont pas en place sur la simulation vierge"
        );
    }

    #[test]
    fn tampered_catalogs_are_refused() {
        let bad_key = r#"[[registry]]
key = 'SYSTEM\CurrentControlSet\Services\WinDefend'
value = "Start"
data = 4
label = "x"
category = "telemetrie"
level = "recommande"
why = "x""#;
        assert!(Catalog::parse(bad_key).is_err(), "registre hors liste");
        let protected_service = r#"[[firewall]]
id = "wu"
service = "wuauserv"
label = "x"
category = "telemetrie"
level = "recommande"
why = "x""#;
        assert!(
            Catalog::parse(protected_service).is_err(),
            "Windows Update jamais bloqué"
        );
        let foreign_program = r#"[[firewall]]
id = "game"
program = 'C:\Games\game.exe'
label = "x"
category = "telemetrie"
level = "recommande"
why = "x""#;
        assert!(
            Catalog::parse(foreign_program).is_err(),
            "seuls les programmes de Windows"
        );
        let strict_without_cost = r#"[[registry]]
key = 'SOFTWARE\Policies\Microsoft\Windows\System'
value = "X"
data = 0
label = "x"
category = "telemetrie"
level = "strict"
why = "x""#;
        assert!(Catalog::parse(strict_without_cost).is_err());
        // Sous-clé d'une clé exacte autorisée : refusée.
        assert!(!registry_allowed(
            "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Advanced\\Other"
        ));
    }
}
