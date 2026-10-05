//! Allègement de Windows : services et stratégies, réversibles.
//!
//! Le catalogue (`config/allegement.toml`) dit quoi changer ; la plateforme lit
//! l'état actuel et l'applique. Le journal garde **la valeur d'origine** : appliquer
//! deux fois ne l'écrase jamais, et `restore` remet exactement ce qu'il y avait.

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const ALLEGEMENT_TOML: &str = include_str!("../../../config/allegement.toml");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Tier {
    Sur,
    Avance,
    /// Réglages orientés jeu (Mode Jeu Windows, GPU, souris…).
    Jeu,
    /// Allègement maximal : services d'arrière-plan en démarrage à la demande, Widgets,
    /// Copilot, applis préinstallées retirées. Remis le temps d'une partie protégée par
    /// un anti-cheat noyau.
    Extreme,
}

impl Tier {
    pub fn parse(s: &str) -> Option<Tier> {
        match s {
            "sur" | "sûr" => Some(Tier::Sur),
            "avance" | "avancé" => Some(Tier::Avance),
            "jeu" => Some(Tier::Jeu),
            "extreme" | "extrême" => Some(Tier::Extreme),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Tier::Sur => "sûr",
            Tier::Avance => "avancé",
            Tier::Jeu => "jeu",
            Tier::Extreme => "extrême",
        }
    }
}

impl StartType {
    pub fn label_fr(self) -> &'static str {
        match self {
            StartType::Boot => "Démarrage noyau",
            StartType::System => "Système",
            StartType::Auto => "Auto",
            StartType::AutoDelayed => "Auto (différé)",
            StartType::Manual => "Manuel",
            StartType::Disabled => "Désactivé",
        }
    }
}

/// Mode de démarrage d'un service Windows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartType {
    Boot,
    System,
    Auto,
    AutoDelayed,
    Manual,
    Disabled,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Protected {
    pub reason: String,
    pub services: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceEntry {
    pub name: String,
    pub label: String,
    pub tier: Tier,
    pub start: StartType,
    pub why: String,
}

/// Appli préinstallée (paquet du Store) retirée au niveau Extrême, réinstallable.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppEntry {
    /// Nom du paquet (`Microsoft.BingNews`).
    pub package: String,
    pub label: String,
    /// Identifiant du Store, pour la réinstaller (`winget install --source msstore`).
    pub store_id: String,
    pub tier: Tier,
    pub why: String,
}

/// Ruche du registre.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Hive {
    #[default]
    Hklm,
    Hkcu,
}

impl Hive {
    pub fn prefix(self) -> &'static str {
        match self {
            Hive::Hklm => "HKLM",
            Hive::Hkcu => "HKCU",
        }
    }
}

/// Donnée d'une valeur de registre : DWORD ou chaîne.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RegData {
    Dword(u32),
    Text(String),
}

impl std::fmt::Display for RegData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegData::Dword(d) => write!(f, "{d}"),
            RegData::Text(t) => write!(f, "\"{t}\""),
        }
    }
}

/// Seules ces clés (chemin complet, en minuscules) peuvent être écrites : les
/// stratégies officielles et quelques réglages utilisateur documentés. Tout le reste
/// du registre est hors de portée, même avec un catalogue modifié à la main.
pub const REGISTRY_ALLOWLIST: [&str; 7] = [
    "hklm\\software\\policies\\",
    "hkcu\\software\\policies\\",
    "hklm\\system\\currentcontrolset\\control\\graphicsdrivers",
    "hkcu\\software\\microsoft\\gamebar",
    "hkcu\\software\\microsoft\\directx\\usergpupreferences",
    "hkcu\\control panel\\mouse",
    // Optimisations plein écran d'un exécutable (Propriétés > Compatibilité) : page Jeux.
    "hkcu\\software\\microsoft\\windows nt\\currentversion\\appcompatflags\\layers",
];

pub fn registry_allowed(full_key: &str) -> bool {
    let k = full_key.to_ascii_lowercase();
    REGISTRY_ALLOWLIST.iter().any(|a| k.starts_with(a))
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyEntry {
    #[serde(default)]
    pub hive: Hive,
    /// Sous-clé dans la ruche.
    pub key: String,
    pub value: String,
    pub data: RegData,
    /// Prise en compte au prochain redémarrage ou à la prochaine ouverture de session.
    #[serde(default)]
    pub reboot: bool,
    pub label: String,
    pub tier: Tier,
    pub why: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub protected: Vec<Protected>,
    #[serde(default)]
    pub services: Vec<ServiceEntry>,
    #[serde(default, rename = "registry")]
    pub policies: Vec<PolicyEntry>,
    #[serde(default)]
    pub tasks: Vec<TaskEntry>,
    /// Dossiers de tâches planifiées jamais touchés.
    #[serde(default)]
    pub protected_tasks: Vec<String>,
    #[serde(default)]
    pub apps: Vec<AppEntry>,
    /// Paquets jamais retirés (début du nom) : Store, Xbox, runtimes, sécurité…
    #[serde(default)]
    pub protected_apps: Vec<String>,
}

/// Tâche planifiée à désactiver (chemin complet du Planificateur : `\Microsoft\…`).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskEntry {
    pub path: String,
    pub label: String,
    pub tier: Tier,
    pub why: String,
}

impl PolicyEntry {
    pub fn full_key(&self) -> String {
        format!("{}\\{}", self.hive.prefix(), self.key)
    }
}

impl Catalog {
    /// Raison de protection d'une tâche planifiée (dossier protégé), insensible à la casse.
    pub fn task_protection(&self, path: &str) -> Option<&'static str> {
        let p = path.to_ascii_lowercase();
        self.protected_tasks
            .iter()
            .any(|prefix| p.starts_with(&prefix.to_ascii_lowercase()))
            .then_some("dossier de tâches protégé (mises à jour, sécurité, TPM, heure…)")
    }

    /// Raison de protection d'un paquet, insensible à la casse.
    pub fn app_protection(&self, package: &str) -> Option<&'static str> {
        let p = package.to_ascii_lowercase();
        self.protected_apps
            .iter()
            .any(|prefix| p.starts_with(&prefix.to_ascii_lowercase()))
            .then_some("paquet protégé (Store, Xbox, runtimes, sécurité, shell)")
    }

    pub fn parse(text: &str) -> Result<Catalog, String> {
        let c: Catalog = toml::from_str(text).map_err(|e| format!("catalogue d'allègement illisible : {e}"))?;
        c.validate()?;
        Ok(c)
    }

    pub fn builtin() -> Catalog {
        Catalog::parse(ALLEGEMENT_TOML).expect("le catalogue embarqué est validé par les tests")
    }

    /// Raison de protection d'un service, s'il est protégé (noms insensibles à la casse,
    /// comme sous Windows).
    pub fn protection(&self, service: &str) -> Option<&str> {
        self.protected
            .iter()
            .find(|p| p.services.iter().any(|s| s.eq_ignore_ascii_case(service)))
            .map(|p| p.reason.as_str())
    }

    pub fn validate(&self) -> Result<(), String> {
        let mut errors = Vec::new();
        let mut seen = HashSet::new();
        for s in &self.services {
            if let Some(reason) = self.protection(&s.name) {
                errors.push(format!("service {} protégé ({reason}) : interdit au catalogue", s.name));
            }
            if !seen.insert(s.name.to_ascii_lowercase()) {
                errors.push(format!("service en double : {}", s.name));
            }
            if !matches!(s.start, StartType::Disabled | StartType::Manual) {
                errors.push(format!("service {} : seul « disabled » ou « manual » allège", s.name));
            }
            if s.why.trim().is_empty() {
                errors.push(format!("service {} : dire pourquoi (why)", s.name));
            }
        }
        let mut seen = HashSet::new();
        for p in &self.policies {
            if !registry_allowed(&p.full_key()) {
                errors.push(format!(
                    "réglage {} : clé {} hors de la liste autorisée",
                    p.value,
                    p.full_key()
                ));
            }
            if !seen.insert((p.key.to_ascii_lowercase(), p.value.to_ascii_lowercase())) {
                errors.push(format!("stratégie en double : {}\\{}", p.key, p.value));
            }
            if p.why.trim().is_empty() {
                errors.push(format!("stratégie {} : dire pourquoi (why)", p.value));
            }
        }
        let mut seen = HashSet::new();
        for t in &self.tasks {
            if !t.path.starts_with('\\') {
                errors.push(format!("tâche {} : chemin complet attendu (\\Microsoft\\…)", t.path));
            }
            if self.task_protection(&t.path).is_some() {
                errors.push(format!("tâche {} : dossier protégé, interdite au catalogue", t.path));
            }
            if !seen.insert(t.path.to_ascii_lowercase()) {
                errors.push(format!("tâche en double : {}", t.path));
            }
        }
        let mut seen = HashSet::new();
        for a in &self.apps {
            if self.app_protection(&a.package).is_some() {
                errors.push(format!("appli {} : paquet protégé, interdit au catalogue", a.package));
            }
            if !seen.insert(a.package.to_ascii_lowercase()) {
                errors.push(format!("appli en double : {}", a.package));
            }
            if a.store_id.trim().is_empty() || a.why.trim().is_empty() {
                errors.push(format!(
                    "appli {} : identifiant du Store et raison (why) exigés",
                    a.package
                ));
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("\n"))
        }
    }
}

/// Changement à appliquer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "do", rename_all = "snake_case")]
pub enum Change {
    ServiceStart {
        name: String,
        to: StartType,
    },
    /// `key` : chemin complet, ruche comprise (`HKCU\\Software\\…`).
    Policy {
        key: String,
        value: String,
        data: RegData,
    },
    /// Active (`true`) ou désactive une tâche planifiée.
    Task {
        path: String,
        enabled: bool,
    },
    /// Retire une appli préinstallée (pour l'utilisateur).
    RemoveApp {
        package: String,
        store_id: String,
    },
}

impl Change {
    pub fn describe(&self) -> String {
        match self {
            Change::ServiceStart { name, to } => format!("service {name} -> {to:?}"),
            Change::Policy { key, value, data } => format!("stratégie {key}\\{value} = {data}"),
            Change::Task { path, enabled } => {
                format!("tâche {path} {}", if *enabled { "activée" } else { "désactivée" })
            }
            Change::RemoveApp { package, .. } => format!("appli {package} retirée"),
        }
    }

    pub fn key(&self) -> String {
        match self {
            Change::ServiceStart { name, .. } => format!("svc:{}", name.to_ascii_lowercase()),
            Change::RemoveApp { package, .. } => format!("app:{}", package.to_ascii_lowercase()),
            Change::Task { path, .. } => format!("task:{}", path.to_ascii_lowercase()),
            Change::Policy { key, value, .. } => {
                format!("pol:{}\\{}", key.to_ascii_lowercase(), value.to_ascii_lowercase())
            }
        }
    }
}

/// Valeur d'origine, pour revenir en arrière.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "undo", rename_all = "snake_case")]
pub enum Original {
    ServiceStart {
        name: String,
        was: StartType,
    },
    /// `was: None` : la valeur n'existait pas, on la supprimera.
    Policy {
        key: String,
        value: String,
        was: Option<RegData>,
    },
    Task {
        path: String,
        was: bool,
    },
    /// L'appli était installée : on la réinstalle depuis le Store.
    App {
        package: String,
        store_id: String,
    },
}

impl Original {
    fn key(&self) -> String {
        match self {
            Original::ServiceStart { name, .. } => format!("svc:{}", name.to_ascii_lowercase()),
            Original::App { package, .. } => format!("app:{}", package.to_ascii_lowercase()),
            Original::Task { path, .. } => format!("task:{}", path.to_ascii_lowercase()),
            Original::Policy { key, value, .. } => {
                format!("pol:{}\\{}", key.to_ascii_lowercase(), value.to_ascii_lowercase())
            }
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Original::ServiceStart { name, was } => format!("service {name} <- {was:?}"),
            Original::Policy {
                key,
                value,
                was: Some(d),
            } => format!("stratégie {key}\\{value} <- {d}"),
            Original::Policy { key, value, was: None } => format!("stratégie {key}\\{value} supprimée"),
            Original::Task { path, was } => {
                format!("tâche {path} <- {}", if *was { "activée" } else { "désactivée" })
            }
            Original::App { package, .. } => format!("appli {package} remise"),
        }
    }
}

/// Lecture et écriture de la configuration système (Windows, ou simulation).
pub trait SystemConfig {
    /// `Ok(None)` : le service n'existe pas sur cette machine.
    fn service_start(&mut self, name: &str) -> Result<Option<StartType>, String>;
    fn set_service_start(&mut self, name: &str, to: StartType) -> Result<(), String>;
    /// `key` : chemin complet, ruche comprise (`HKLM\\…` ou `HKCU\\…`).
    fn policy(&mut self, key: &str, value: &str) -> Result<Option<RegData>, String>;
    fn set_policy(&mut self, key: &str, value: &str, data: &RegData) -> Result<(), String>;
    fn delete_policy(&mut self, key: &str, value: &str) -> Result<(), String>;
    /// `Ok(None)` : la tâche n'existe pas sur cette machine.
    fn task_enabled(&mut self, path: &str) -> Result<Option<bool>, String>;
    fn set_task_enabled(&mut self, path: &str, enabled: bool) -> Result<(), String>;
    /// L'appli (nom de paquet) est-elle installée pour l'utilisateur ?
    fn app_installed(&mut self, package: &str) -> Result<bool, String>;
    fn remove_app(&mut self, package: &str) -> Result<(), String>;
    /// Remet l'appli : réenregistrée depuis la copie restée sur le disque (paquet
    /// provisionné), sinon réinstallée depuis le Store.
    fn install_app(&mut self, package: &str, store_id: &str) -> Result<(), String>;
}

pub fn plan(catalog: &Catalog, tiers: &[Tier]) -> Vec<Change> {
    let services = catalog
        .services
        .iter()
        .filter(|s| tiers.contains(&s.tier))
        .map(|s| Change::ServiceStart {
            name: s.name.clone(),
            to: s.start,
        });
    let policies = catalog
        .policies
        .iter()
        .filter(|p| tiers.contains(&p.tier))
        .map(|p| Change::Policy {
            key: p.full_key(),
            value: p.value.clone(),
            data: p.data.clone(),
        });
    let tasks = catalog
        .tasks
        .iter()
        .filter(|t| tiers.contains(&t.tier))
        .map(|t| Change::Task {
            path: t.path.clone(),
            enabled: false,
        });
    let apps = catalog
        .apps
        .iter()
        .filter(|a| tiers.contains(&a.tier))
        .map(|a| Change::RemoveApp {
            package: a.package.clone(),
            store_id: a.store_id.clone(),
        });
    services.chain(policies).chain(tasks).chain(apps).collect()
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllegeJournal {
    pub originals: Vec<Original>,
}

impl AllegeJournal {
    pub fn load(path: &PathBuf) -> Result<AllegeJournal, String> {
        match fs::read(path) {
            Ok(b) => serde_json::from_slice(&b)
                .map_err(|e| format!("journal d'allègement illisible {} : {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(AllegeJournal::default()),
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
pub struct AllegeReport {
    pub done: Vec<String>,
    /// Déjà dans l'état voulu, ou absent de cette machine.
    pub unchanged: Vec<String>,
    pub failed: Vec<String>,
}

/// Applique les changements. Chaque valeur d'origine est ajoutée au journal (une
/// seule fois par réglage) et le journal est sauvegardé après chaque changement.
pub fn apply(
    sys: &mut dyn SystemConfig,
    catalog: &Catalog,
    changes: &[Change],
    journal: &mut AllegeJournal,
    save: &mut dyn FnMut(&AllegeJournal) -> Result<(), String>,
) -> AllegeReport {
    let mut r = AllegeReport::default();
    for c in changes {
        let what = c.describe();
        let original = match c {
            Change::ServiceStart { name, to } => {
                // Dernière barrière : même un catalogue modifié à la main ne peut pas
                // toucher un service protégé.
                if let Some(reason) = catalog.protection(name) {
                    r.failed.push(format!("{what} — refusé, service protégé : {reason}"));
                    continue;
                }
                match sys.service_start(name) {
                    Err(e) => Err(e),
                    Ok(None) => {
                        r.unchanged.push(format!("{what} — absent de cette machine"));
                        continue;
                    }
                    Ok(Some(was)) if was == *to || (was == StartType::Disabled && *to == StartType::Manual) => {
                        r.unchanged.push(format!("{what} — déjà fait"));
                        continue;
                    }
                    Ok(Some(was)) => sys.set_service_start(name, *to).map(|()| Original::ServiceStart {
                        name: name.clone(),
                        was,
                    }),
                }
            }
            Change::Policy { key, value, data } => match sys.policy(key, value) {
                Err(e) => Err(e),
                Ok(Some(d)) if d == *data => {
                    r.unchanged.push(format!("{what} — déjà fait"));
                    continue;
                }
                Ok(was) => sys.set_policy(key, value, data).map(|()| Original::Policy {
                    key: key.clone(),
                    value: value.clone(),
                    was,
                }),
            },
            Change::Task { path, enabled } => {
                if let Some(reason) = catalog.task_protection(path) {
                    r.failed.push(format!("{what} — refusé, tâche protégée : {reason}"));
                    continue;
                }
                match sys.task_enabled(path) {
                    Err(e) => Err(e),
                    Ok(None) => {
                        r.unchanged.push(format!("{what} — absente de cette machine"));
                        continue;
                    }
                    Ok(Some(was)) if was == *enabled => {
                        r.unchanged.push(format!("{what} — déjà fait"));
                        continue;
                    }
                    Ok(Some(was)) => sys.set_task_enabled(path, *enabled).map(|()| Original::Task {
                        path: path.clone(),
                        was,
                    }),
                }
            }
            Change::RemoveApp { package, store_id } => {
                if let Some(reason) = catalog.app_protection(package) {
                    r.failed.push(format!("{what} — refusé : {reason}"));
                    continue;
                }
                match sys.app_installed(package) {
                    Err(e) => Err(e),
                    Ok(false) => {
                        r.unchanged.push(format!("{what} — absente de cette machine"));
                        continue;
                    }
                    Ok(true) => sys.remove_app(package).map(|()| Original::App {
                        package: package.clone(),
                        store_id: store_id.clone(),
                    }),
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

/// Remet toutes les valeurs d'origine, de la plus récente à la plus ancienne. Le
/// journal ne garde que ce qui n'a pas pu être restauré.
pub fn restore(sys: &mut dyn SystemConfig, journal: &mut AllegeJournal) -> AllegeReport {
    let mut r = AllegeReport::default();
    let mut kept = Vec::new();
    for o in journal.originals.iter().rev() {
        let res = match o {
            Original::ServiceStart { name, was } => sys.set_service_start(name, *was),
            Original::Policy {
                key,
                value,
                was: Some(d),
            } => sys.set_policy(key, value, d),
            Original::Policy { key, value, was: None } => sys.delete_policy(key, value),
            Original::Task { path, was } => sys.set_task_enabled(path, *was),
            Original::App { package, store_id } => sys.install_app(package, store_id),
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

/// Un service Windows, pour la page Services.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceInfo {
    pub name: String,
    pub display: String,
    /// `None` : illisible.
    pub start: Option<StartType>,
    pub running: bool,
    /// Service par utilisateur (instance `Nom_1a2b3`) : réglé par son modèle, pas ici.
    pub per_user: bool,
}

/// Services d'éditeurs tiers qui ne servent qu'à se mettre à jour ou à précharger une
/// appli : à la demande, ils redémarrent quand l'appli en a besoin. (Page Services :
/// « superflus », avec ceux du catalogue.)
pub const TIERS_SUPERFLUS: [(&str, &str); 14] = [
    ("LGHUBUpdaterService", "Mise à jour de Logitech G Hub"),
    ("AdobeARMservice", "Mise à jour d'Adobe Reader"),
    ("GoogleUpdaterService", "Mise à jour de Google Chrome"),
    ("GoogleUpdaterInternalService", "Mise à jour de Google Chrome"),
    ("gupdate", "Mise à jour de Google"),
    ("gupdatem", "Mise à jour de Google"),
    ("brave", "Mise à jour de Brave"),
    ("bravem", "Mise à jour de Brave"),
    ("BraveElevationService", "Élévation de Brave (mises à jour)"),
    ("MozillaMaintenance", "Mise à jour de Firefox"),
    (
        "ClickToRunSvc",
        "Office « Démarrer en un clic » (préchargement d'Office)",
    ),
    ("Razer Game Manager Service", "Service de jeux Razer"),
    ("CorsairService", "Service iCUE (Corsair)"),
    ("ArmouryCrateService", "Service Armoury Crate (Asus)"),
];

/// Raison pour laquelle un service est superflu (catalogue de Prism ou éditeur tiers).
pub fn superflu(catalog: &Catalog, name: &str) -> Option<(String, StartType)> {
    if let Some(s) = catalog.services.iter().find(|s| s.name.eq_ignore_ascii_case(name)) {
        return Some((s.why.clone(), s.start));
    }
    TIERS_SUPERFLUS
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, why)| (why.to_string(), StartType::Manual))
}

/// Règle un service quelconque (page Services) au mode exact demandé — y compris
/// Désactivé → Manuel, que l'allègement considère déjà fait. Refusé pour un service
/// protégé ; la valeur d'origine est journalisée une seule fois (« Tout restaurer » et
/// `restore_keys` la remettent).
pub fn set_service(
    sys: &mut dyn SystemConfig,
    catalog: &Catalog,
    journal: &mut AllegeJournal,
    name: &str,
    to: StartType,
    save: &mut dyn FnMut(&AllegeJournal) -> Result<(), String>,
) -> Result<String, String> {
    if let Some(reason) = catalog.protection(name) {
        return Err(format!("{name} est protégé : {reason}"));
    }
    if !matches!(
        to,
        StartType::Auto | StartType::AutoDelayed | StartType::Manual | StartType::Disabled
    ) {
        return Err("seuls Auto, Auto (différé), Manuel et Désactivé se règlent ici".into());
    }
    let was = sys
        .service_start(name)?
        .ok_or_else(|| format!("{name} : service absent"))?;
    if was == to {
        return Ok(format!("{name} : déjà {}", to.label_fr()));
    }
    sys.set_service_start(name, to)?;
    let key = format!("svc:{}", name.to_ascii_lowercase());
    if !journal.has(&key) {
        journal.originals.push(Original::ServiceStart {
            name: name.to_string(),
            was,
        });
        save(journal)?;
    }
    Ok(format!("{name} : {} → {}", was.label_fr(), to.label_fr()))
}

/// Changement du catalogue identifié par sa clé (`svc:dps`, `app:msteams`…), quel que
/// soit son niveau : pour appliquer un seul élément.
pub fn change_for(catalog: &Catalog, key: &str) -> Option<Change> {
    plan(catalog, &[Tier::Sur, Tier::Avance, Tier::Jeu, Tier::Extreme])
        .into_iter()
        .find(|c| c.key() == key)
}

/// Remet seulement les éléments de ces clés ; les autres restent au journal.
pub fn restore_keys(sys: &mut dyn SystemConfig, journal: &mut AllegeJournal, keys: &[String]) -> AllegeReport {
    let (mine, others): (Vec<Original>, Vec<Original>) =
        journal.originals.drain(..).partition(|o| keys.contains(&o.key()));
    journal.originals = others;
    let mut partial = AllegeJournal { originals: mine };
    let r = restore(sys, &mut partial);
    journal.originals.extend(partial.originals);
    r
}

/// Prism a-t-il changé cet élément (valeur d'origine au journal) ?
pub fn journaled(journal: &AllegeJournal, key: &str) -> bool {
    journal.has(key)
}

/// Simulation pour les tests et la démonstration.
#[derive(Clone, Debug, Default)]
pub struct MockSystem {
    pub services: std::collections::BTreeMap<String, StartType>,
    pub policies: std::collections::BTreeMap<(String, String), RegData>,
    /// Tâches connues : chemin (minuscules) -> activée.
    pub tasks: std::collections::BTreeMap<String, bool>,
    /// Paquets installés (minuscules) et identifiant du Store de chacun.
    pub apps: std::collections::BTreeMap<String, String>,
    pub writes: usize,
}

impl SystemConfig for MockSystem {
    fn app_installed(&mut self, package: &str) -> Result<bool, String> {
        Ok(self.apps.contains_key(&package.to_ascii_lowercase()))
    }
    fn remove_app(&mut self, package: &str) -> Result<(), String> {
        self.writes += 1;
        self.apps.remove(&package.to_ascii_lowercase());
        Ok(())
    }
    fn install_app(&mut self, package: &str, store_id: &str) -> Result<(), String> {
        self.writes += 1;
        self.apps.insert(package.to_ascii_lowercase(), store_id.into());
        Ok(())
    }
    fn task_enabled(&mut self, path: &str) -> Result<Option<bool>, String> {
        Ok(self.tasks.get(&path.to_ascii_lowercase()).copied())
    }
    fn set_task_enabled(&mut self, path: &str, enabled: bool) -> Result<(), String> {
        self.writes += 1;
        self.tasks.insert(path.to_ascii_lowercase(), enabled);
        Ok(())
    }
    fn service_start(&mut self, name: &str) -> Result<Option<StartType>, String> {
        Ok(self.services.get(&name.to_ascii_lowercase()).copied())
    }
    fn set_service_start(&mut self, name: &str, to: StartType) -> Result<(), String> {
        self.writes += 1;
        self.services.insert(name.to_ascii_lowercase(), to);
        Ok(())
    }
    fn policy(&mut self, key: &str, value: &str) -> Result<Option<RegData>, String> {
        Ok(self
            .policies
            .get(&(key.to_ascii_lowercase(), value.to_ascii_lowercase()))
            .cloned())
    }
    fn set_policy(&mut self, key: &str, value: &str, data: &RegData) -> Result<(), String> {
        self.writes += 1;
        self.policies
            .insert((key.to_ascii_lowercase(), value.to_ascii_lowercase()), data.clone());
        Ok(())
    }
    fn delete_policy(&mut self, key: &str, value: &str) -> Result<(), String> {
        self.writes += 1;
        self.policies
            .remove(&(key.to_ascii_lowercase(), value.to_ascii_lowercase()));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un Windows 11 typique : les services du catalogue existent avec leurs
    /// réglages d'usine, aucune stratégie posée.
    fn stock_windows(c: &Catalog) -> MockSystem {
        let mut m = MockSystem::default();
        for s in &c.services {
            let factory = match s.name.as_str() {
                "WSearch" => StartType::AutoDelayed,
                "DiagTrack" | "SysMain" | "Spooler" => StartType::Auto,
                "RemoteRegistry" => StartType::Disabled,
                _ => StartType::Manual,
            };
            m.services.insert(s.name.to_ascii_lowercase(), factory);
        }
        m.services.remove("wmpnetworksvc"); // absent de cette installation
        m.policies.insert(
            (
                r"hklm\software\policies\microsoft\edge".into(),
                "startupboostenabled".into(),
            ),
            RegData::Dword(1),
        );
        m
    }

    fn run(m: &mut MockSystem, c: &Catalog, tiers: &[Tier], j: &mut AllegeJournal) -> AllegeReport {
        apply(m, c, &plan(c, tiers), j, &mut |_| Ok(()))
    }

    #[test]
    fn extreme_removes_preinstalled_apps_and_restore_reinstalls_them_from_the_store() {
        let c = Catalog::builtin();
        let mut m = stock_windows(&c);
        m.apps.insert("microsoft.bingnews".into(), "9WZDNCRFHVFW".into());
        m.apps.insert("microsoft.windowsstore".into(), "x".into());
        let mut j = AllegeJournal::default();
        // Les niveaux sûr/avancé ne touchent à aucune appli.
        run(&mut m, &c, &[Tier::Sur, Tier::Avance], &mut j);
        assert!(m.apps.contains_key("microsoft.bingnews"));
        let r = run(&mut m, &c, &[Tier::Extreme], &mut j);
        assert!(!m.apps.contains_key("microsoft.bingnews"));
        assert!(m.apps.contains_key("microsoft.windowsstore"));
        assert!(r.done.iter().any(|d| d.contains("Microsoft.BingNews")));
        restore(&mut m, &mut j);
        assert!(m.apps.contains_key("microsoft.bingnews"));
        assert!(j.originals.is_empty());
    }

    #[test]
    fn one_entry_can_be_applied_and_put_back_alone() {
        let c = Catalog::builtin();
        let mut m = stock_windows(&c);
        let mut j = AllegeJournal::default();
        run(&mut m, &c, &[Tier::Sur], &mut j);
        let sysmain = change_for(&c, "svc:sysmain").unwrap();
        apply(&mut m, &c, &[sysmain], &mut j, &mut |_| Ok(()));
        assert_eq!(m.services["sysmain"], StartType::Disabled);
        // Seul SysMain revient ; la télémétrie reste coupée.
        restore_keys(&mut m, &mut j, &["svc:sysmain".into()]);
        assert_eq!(m.services["sysmain"], StartType::Auto);
        assert_eq!(m.services["diagtrack"], StartType::Disabled);
        assert!(!journaled(&j, "svc:sysmain") && journaled(&j, "svc:diagtrack"));
        assert!(change_for(&c, "svc:wuauserv").is_none());
    }

    #[test]
    fn any_service_can_be_set_exactly_except_protected_ones() {
        let c = Catalog::builtin();
        let mut m = MockSystem::default();
        m.services.insert("spooler".into(), StartType::Disabled);
        m.services.insert("wuauserv".into(), StartType::Manual);
        let mut j = AllegeJournal::default();
        // Désactivé -> Manuel : appliqué (l'allègement l'aurait jugé déjà fait).
        set_service(&mut m, &c, &mut j, "Spooler", StartType::Manual, &mut |_| Ok(())).unwrap();
        assert_eq!(m.services["spooler"], StartType::Manual);
        set_service(&mut m, &c, &mut j, "Spooler", StartType::Auto, &mut |_| Ok(())).unwrap();
        // L'origine journalisée reste la toute première.
        restore_keys(&mut m, &mut j, &["svc:spooler".into()]);
        assert_eq!(m.services["spooler"], StartType::Disabled);
        assert!(set_service(&mut m, &c, &mut j, "wuauserv", StartType::Disabled, &mut |_| Ok(())).is_err());
        assert_eq!(m.services["wuauserv"], StartType::Manual);
    }

    #[test]
    fn protected_app_is_refused_even_if_planned_by_hand() {
        let c = Catalog::builtin();
        let mut m = MockSystem::default();
        m.apps.insert("microsoft.gamingapp".into(), "x".into());
        let changes = [Change::RemoveApp {
            package: "Microsoft.GamingApp".into(),
            store_id: "9MV0B5HZVK9Z".into(),
        }];
        let r = apply(&mut m, &c, &changes, &mut AllegeJournal::default(), &mut |_| Ok(()));
        assert_eq!(r.failed.len(), 1);
        assert!(m.apps.contains_key("microsoft.gamingapp"));
    }

    #[test]
    fn builtin_catalog_is_valid_and_never_touches_a_protected_service() {
        let c = Catalog::builtin();
        assert!(c.services.len() >= 15 && c.policies.len() >= 8);
        for s in &c.services {
            assert!(c.protection(&s.name).is_none(), "{} est protégé", s.name);
        }
    }

    #[test]
    fn anticheat_critical_services_are_protected() {
        let c = Catalog::builtin();
        for name in [
            "CryptSvc",
            "Winmgmt",
            "tbs",
            "wuauserv",
            "W32Time",
            "vgc",
            "EasyAntiCheat_EOS",
            "BEService",
            "cryptsvc",
        ] {
            assert!(c.protection(name).is_some(), "{name} doit être protégé");
        }
    }

    #[test]
    fn a_catalog_that_targets_a_protected_service_is_rejected() {
        let bad = ALLEGEMENT_TOML.replace("name = \"Fax\"", "name = \"CryptSvc\"");
        assert!(Catalog::parse(&bad).unwrap_err().contains("CryptSvc"));
        let bad = ALLEGEMENT_TOML.replace(
            r"key = 'SOFTWARE\Policies\Microsoft\Windows\AdvertisingInfo'",
            r"key = 'SYSTEM\CurrentControlSet\Services\vgk'",
        );
        assert!(Catalog::parse(&bad).is_err(), "hors SOFTWARE\\Policies refusé");
    }

    #[test]
    fn protected_service_is_refused_even_if_planned_by_hand() {
        let c = Catalog::builtin();
        let mut m = MockSystem::default();
        m.services.insert("winmgmt".into(), StartType::Auto);
        let changes = vec![Change::ServiceStart {
            name: "Winmgmt".into(),
            to: StartType::Disabled,
        }];
        let r = apply(&mut m, &c, &changes, &mut AllegeJournal::default(), &mut |_| Ok(()));
        assert_eq!(r.failed.len(), 1);
        assert_eq!(m.services["winmgmt"], StartType::Auto);
    }

    #[test]
    fn safe_tier_only_by_default() {
        let c = Catalog::builtin();
        let mut m = stock_windows(&c);
        let mut j = AllegeJournal::default();
        let r = run(&mut m, &c, &[Tier::Sur], &mut j);
        assert!(r.failed.is_empty(), "{:?}", r.failed);
        assert_eq!(m.services["diagtrack"], StartType::Disabled);
        assert_eq!(m.services["wsearch"], StartType::AutoDelayed, "avancé non appliqué");
        assert!(r
            .unchanged
            .iter()
            .any(|u| u.contains("WMPNetworkSvc") && u.contains("absent")));
        assert!(r
            .unchanged
            .iter()
            .any(|u| u.contains("RemoteRegistry") && u.contains("déjà")));
    }

    #[test]
    fn restore_puts_back_exactly_the_factory_state() {
        let c = Catalog::builtin();
        let factory = stock_windows(&c);
        let mut m = factory.clone();
        let mut j = AllegeJournal::default();
        run(&mut m, &c, &[Tier::Sur, Tier::Avance, Tier::Jeu], &mut j);
        assert_ne!(m.services, factory.services);
        assert_eq!(
            m.policies
                .get(&(r"hkcu\control panel\mouse".into(), "mousespeed".into())),
            Some(&RegData::Text("0".into())),
            "valeur texte écrite"
        );
        let r = restore(&mut m, &mut j);
        assert!(r.failed.is_empty());
        assert_eq!(m.services, factory.services, "services, y compris démarrage différé");
        assert_eq!(
            m.policies, factory.policies,
            "stratégies absentes supprimées, existantes remises"
        );
        assert!(j.originals.is_empty());
    }

    #[test]
    fn applying_twice_never_overwrites_the_original() {
        let c = Catalog::builtin();
        let factory = stock_windows(&c);
        let mut m = factory.clone();
        let mut j = AllegeJournal::default();
        run(&mut m, &c, &[Tier::Sur], &mut j);
        let second = run(&mut m, &c, &[Tier::Sur], &mut j);
        assert!(second.done.is_empty(), "rien à refaire : {:?}", second.done);
        // Même si l'utilisateur a remis un service entre-temps, l'original reste celui d'usine.
        m.services.insert("diagtrack".into(), StartType::Manual);
        run(&mut m, &c, &[Tier::Sur], &mut j);
        restore(&mut m, &mut j);
        assert_eq!(m.services["diagtrack"], StartType::Auto);
    }

    #[test]
    fn journal_is_saved_after_each_change() {
        let c = Catalog::builtin();
        let mut m = stock_windows(&c);
        let mut j = AllegeJournal::default();
        let mut saves = 0;
        let r = apply(&mut m, &c, &plan(&c, &[Tier::Sur]), &mut j, &mut |_| {
            saves += 1;
            Ok(())
        });
        assert_eq!(saves, r.done.len());
        assert_eq!(j.originals.len(), r.done.len());
    }

    #[test]
    fn registry_outside_the_allowlist_is_rejected() {
        let bad = ALLEGEMENT_TOML.replace(
            r"key = 'Software\Microsoft\GameBar'",
            r"key = 'Software\Microsoft\Windows\CurrentVersion\Run'",
        );
        assert!(Catalog::parse(&bad).unwrap_err().contains("hors de la liste"));
        assert!(registry_allowed(r"HKCU\Control Panel\Mouse"));
        assert!(!registry_allowed(r"HKLM\SYSTEM\CurrentControlSet\Services\vgk"));
    }

    #[test]
    fn game_tier_is_separate_from_the_default_one() {
        let c = Catalog::builtin();
        let jeu = plan(&c, &[Tier::Jeu]);
        assert!(jeu.len() >= 5);
        assert!(plan(&c, &[Tier::Sur]).iter().all(|ch| !jeu.contains(ch)));
    }

    #[test]
    fn telemetry_tasks_are_disabled_then_restored_and_protected_folders_refused() {
        let c = Catalog::builtin();
        assert!(c.tasks.len() >= 10);
        for t in &c.tasks {
            assert!(c.task_protection(&t.path).is_none(), "{}", t.path);
        }
        assert!(c.task_protection(r"\Microsoft\Windows\TPM\Tpm-Maintenance").is_some());
        assert!(c
            .task_protection(r"\microsoft\windows\windowsupdate\Scheduled Start")
            .is_some());

        let mut m = MockSystem::default();
        for t in &c.tasks {
            m.tasks.insert(t.path.to_ascii_lowercase(), true);
        }
        m.tasks.remove(&c.tasks[1].path.to_ascii_lowercase()); // absente sur ce Windows
        let before = m.tasks.clone();
        let mut j = AllegeJournal::default();
        let r = apply(&mut m, &c, &plan(&c, &[Tier::Sur]), &mut j, &mut |_| Ok(()));
        assert!(r.failed.is_empty(), "{:?}", r.failed);
        let appraiser = r"\microsoft\windows\application experience\microsoft compatibility appraiser";
        assert_eq!(m.tasks.get(appraiser), Some(&false));
        assert!(r.unchanged.iter().any(|u| u.contains("absente")));
        restore(&mut m, &mut j);
        assert_eq!(m.tasks, before);
    }

    #[test]
    fn a_catalog_task_in_a_protected_folder_is_rejected() {
        let bad = ALLEGEMENT_TOML.replace(
            r"path = '\Microsoft\Windows\Maps\MapsToastTask'",
            r"path = '\Microsoft\Windows\TPM\Tpm-Maintenance'",
        );
        assert!(Catalog::parse(&bad).unwrap_err().contains("protégé"));
    }

    #[test]
    fn journal_file_round_trip() {
        let path = std::env::temp_dir().join(format!("prism-allege-{}.json", std::process::id()));
        let j = AllegeJournal {
            originals: vec![
                Original::ServiceStart {
                    name: "WSearch".into(),
                    was: StartType::AutoDelayed,
                },
                Original::Policy {
                    key: "SOFTWARE\\Policies\\X".into(),
                    value: "Y".into(),
                    was: None,
                },
            ],
        };
        j.save(&path).unwrap();
        assert_eq!(AllegeJournal::load(&path).unwrap(), j);
        let _ = fs::remove_file(path);
    }
}

/// Compression de la mémoire avant Prism (journal à part : ce n'est ni un service ni une
/// stratégie). `None` dans le fichier : jamais changée par Prism.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompressionAvant {
    pub etait_active: Option<bool>,
}

impl CompressionAvant {
    pub const FICHIER: &'static str = "compression-memoire.json";

    pub fn charger(dir: &std::path::Path) -> CompressionAvant {
        fs::read(dir.join(Self::FICHIER))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn enregistrer(&self, dir: &std::path::Path) -> Result<(), String> {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        fs::write(
            dir.join(Self::FICHIER),
            serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())
    }
}
