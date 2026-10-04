//! Applications lancées au démarrage : liste, recommandations, désactivation
//! réversible (mécanisme `StartupApproved`, celui du Gestionnaire des tâches).

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::glob::any_matches;

pub const DEMARRAGE_TOML: &str = include_str!("../../../config/demarrage.toml");

/// D'où vient l'entrée (chaque source a sa clé `StartupApproved`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    /// HKCU\…\Run
    UserRun,
    /// HKLM\…\Run
    MachineRun,
    /// HKLM\…\WOW6432Node\…\Run (programmes 32 bits)
    MachineRun32,
    /// Dossier Démarrage de l'utilisateur
    UserFolder,
    /// Dossier Démarrage commun
    CommonFolder,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::UserRun => "utilisateur",
            Source::MachineRun => "machine",
            Source::MachineRun32 => "machine 32 bits",
            Source::UserFolder => "dossier utilisateur",
            Source::CommonFolder => "dossier commun",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub source: Source,
    /// Nom de la valeur `Run`, ou nom du fichier du dossier Démarrage.
    pub name: String,
    pub command: String,
    /// Valeur `StartupApproved` actuelle (`None` : absente, donc activée).
    pub approval: Option<Vec<u8>>,
}

/// Octet de tête de `StartupApproved` : pair = activé (02, 06), impair = désactivé (03, 07).
pub fn is_enabled(approval: &Option<Vec<u8>>) -> bool {
    approval.as_ref().and_then(|b| b.first()).map_or(true, |b| b % 2 == 0)
}

/// Valeur écrite pour désactiver : 03 00 00 00 + date de désactivation (FILETIME),
/// comme le Gestionnaire des tâches.
pub fn disabled_value(filetime_now: u64) -> Vec<u8> {
    let mut v = vec![3, 0, 0, 0];
    v.extend_from_slice(&filetime_now.to_le_bytes());
    v
}

/// Valeur « activé » écrite par `prism demarrage on`.
pub fn enabled_value() -> Vec<u8> {
    vec![2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Advice {
    Desactiver,
    Optionnel,
    Garder,
}

impl Advice {
    pub fn label(self) -> &'static str {
        match self {
            Advice::Desactiver => "à désactiver",
            Advice::Optionnel => "optionnel",
            Advice::Garder => "à garder",
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Protected {
    pub reason: String,
    pub patterns: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub patterns: Vec<String>,
    pub advice: Advice,
    pub why: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub protected: Vec<Protected>,
    pub rules: Vec<Rule>,
}

fn matches(patterns: &[String], e: &Entry) -> bool {
    any_matches(patterns, &e.name.to_lowercase()) || any_matches(patterns, &e.command.to_lowercase())
}

impl Catalog {
    pub fn parse(text: &str) -> Result<Catalog, String> {
        let c: Catalog = toml::from_str(text).map_err(|e| format!("catalogue de démarrage illisible : {e}"))?;
        for p in c
            .protected
            .iter()
            .flat_map(|p| &p.patterns)
            .chain(c.rules.iter().flat_map(|r| &r.patterns))
        {
            if p.chars().any(|ch| ch.is_uppercase()) {
                return Err(format!("motif « {p} » : en minuscules"));
            }
        }
        Ok(c)
    }

    pub fn builtin() -> Catalog {
        Catalog::parse(DEMARRAGE_TOML).expect("catalogue embarqué validé par les tests")
    }

    pub fn protection(&self, e: &Entry) -> Option<&str> {
        self.protected
            .iter()
            .find(|p| matches(&p.patterns, e))
            .map(|p| p.reason.as_str())
    }

    /// Premier conseil qui correspond (l'ordre du fichier compte).
    pub fn advice(&self, e: &Entry) -> Option<&Rule> {
        self.rules.iter().find(|r| matches(&r.patterns, e))
    }
}

/// Lecture / écriture des entrées de démarrage (Windows, ou simulation).
pub trait StartupConfig {
    fn entries(&mut self) -> Result<Vec<Entry>, String>;
    /// `None` : supprime la valeur `StartupApproved` (retour à « activé d'origine »).
    fn set_approval(&mut self, source: Source, name: &str, value: Option<&[u8]>) -> Result<(), String>;
    /// Date courante en FILETIME (pour la valeur « désactivé »).
    fn now_filetime(&mut self) -> u64;
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Original {
    pub source: Source,
    pub name: String,
    pub was: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartupJournal {
    pub originals: Vec<Original>,
}

impl StartupJournal {
    pub fn load(path: &PathBuf) -> Result<StartupJournal, String> {
        match fs::read(path) {
            Ok(b) => serde_json::from_slice(&b).map_err(|e| format!("journal de démarrage illisible : {e}")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(StartupJournal::default()),
            Err(e) => Err(format!("{} : {e}", path.display())),
        }
    }

    pub fn save(&self, path: &PathBuf) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        fs::rename(&tmp, path).map_err(|e| e.to_string())
    }

    fn remember(&mut self, e: &Entry) {
        if !self
            .originals
            .iter()
            .any(|o| o.source == e.source && o.name.eq_ignore_ascii_case(&e.name))
        {
            self.originals.push(Original {
                source: e.source,
                name: e.name.clone(),
                was: e.approval.clone(),
            });
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StartupReport {
    pub done: Vec<String>,
    pub unchanged: Vec<String>,
    pub failed: Vec<String>,
}

/// Désactive une entrée (refusé si elle est protégée).
pub fn disable(
    sys: &mut dyn StartupConfig,
    catalog: &Catalog,
    e: &Entry,
    journal: &mut StartupJournal,
    r: &mut StartupReport,
) {
    if let Some(reason) = catalog.protection(e) {
        r.failed.push(format!("{} — refusé, protégé : {reason}", e.name));
        return;
    }
    if !is_enabled(&e.approval) {
        r.unchanged.push(format!("{} — déjà désactivé", e.name));
        return;
    }
    let value = disabled_value(sys.now_filetime());
    match sys.set_approval(e.source, &e.name, Some(&value)) {
        Ok(()) => {
            journal.remember(e);
            r.done.push(format!("{} désactivé ({})", e.name, e.source.label()));
        }
        Err(err) => r.failed.push(format!("{} — {err}", e.name)),
    }
}

/// Réactive une entrée désactivée.
pub fn enable(sys: &mut dyn StartupConfig, e: &Entry, journal: &mut StartupJournal, r: &mut StartupReport) {
    if is_enabled(&e.approval) {
        r.unchanged.push(format!("{} — déjà activé", e.name));
        return;
    }
    match sys.set_approval(e.source, &e.name, Some(&enabled_value())) {
        Ok(()) => {
            journal.remember(e);
            r.done.push(format!("{} activé", e.name));
        }
        Err(err) => r.failed.push(format!("{} — {err}", e.name)),
    }
}

/// Désactive toutes les entrées conseillées « à désactiver ».
pub fn apply_recommended(
    sys: &mut dyn StartupConfig,
    catalog: &Catalog,
    journal: &mut StartupJournal,
) -> Result<StartupReport, String> {
    let mut r = StartupReport::default();
    for e in sys.entries()? {
        if catalog.advice(&e).map(|a| a.advice) == Some(Advice::Desactiver) {
            disable(sys, catalog, &e, journal, &mut r);
        }
    }
    Ok(r)
}

/// Remet les valeurs d'origine ; le journal ne garde que les échecs.
pub fn restore(sys: &mut dyn StartupConfig, journal: &mut StartupJournal) -> StartupReport {
    let mut r = StartupReport::default();
    let mut kept = Vec::new();
    for o in journal.originals.iter().rev() {
        match sys.set_approval(o.source, &o.name, o.was.as_deref()) {
            Ok(()) => r.done.push(format!("{} remis comme à l'origine", o.name)),
            Err(e) => {
                r.failed.push(format!("{} — {e}", o.name));
                kept.push(o.clone());
            }
        }
    }
    kept.reverse();
    journal.originals = kept;
    r
}

/// Simulation pour les tests.
#[derive(Clone, Debug, Default)]
pub struct MockStartup {
    pub items: Vec<Entry>,
}

impl StartupConfig for MockStartup {
    fn entries(&mut self) -> Result<Vec<Entry>, String> {
        Ok(self.items.clone())
    }
    fn set_approval(&mut self, source: Source, name: &str, value: Option<&[u8]>) -> Result<(), String> {
        let e = self
            .items
            .iter_mut()
            .find(|e| e.source == source && e.name.eq_ignore_ascii_case(name))
            .ok_or("entrée absente")?;
        e.approval = value.map(<[u8]>::to_vec);
        Ok(())
    }
    fn now_filetime(&mut self) -> u64 {
        0x01DC_0000_0000_0000
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(source: Source, name: &str, command: &str) -> Entry {
        Entry {
            source,
            name: name.into(),
            command: command.into(),
            approval: None,
        }
    }

    /// Un PC de joueur typique.
    fn typical_pc() -> MockStartup {
        MockStartup {
            items: vec![
                entry(
                    Source::UserRun,
                    "MicrosoftEdgeAutoLaunch_0F1E2D",
                    r#""C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe" --no-startup-window"#,
                ),
                entry(
                    Source::UserRun,
                    "OneDrive",
                    r#""C:\Program Files\Microsoft OneDrive\OneDrive.exe" /background"#,
                ),
                entry(
                    Source::UserRun,
                    "Spotify",
                    r#""C:\Users\x\AppData\Roaming\Spotify\Spotify.exe" /minimized"#,
                ),
                entry(
                    Source::UserRun,
                    "Steam",
                    r#""C:\Program Files (x86)\Steam\steam.exe" -silent"#,
                ),
                entry(
                    Source::UserRun,
                    "Discord",
                    r#"C:\Users\x\AppData\Local\Discord\Update.exe --processStart Discord.exe"#,
                ),
                entry(
                    Source::MachineRun,
                    "SecurityHealth",
                    r"%windir%\system32\SecurityHealthSystray.exe",
                ),
                entry(
                    Source::MachineRun,
                    "Riot Vanguard",
                    r#""C:\Program Files\Riot Vanguard\vgtray.exe""#,
                ),
                entry(
                    Source::MachineRun,
                    "RtkAudUService",
                    r#""C:\Windows\System32\DriverStore\FileRepository\realtekservice.inf_amd64\RtkAudUService64.exe" -background"#,
                ),
                entry(
                    Source::MachineRun32,
                    "AdobeGCInvoker-1.0",
                    r#""C:\Program Files (x86)\Common Files\Adobe\AdobeGCClient\AGCInvokerUtility.exe""#,
                ),
                entry(
                    Source::UserFolder,
                    "Teams.lnk",
                    r"C:\Users\x\AppData\Roaming\Microsoft\Windows\Start Menu\Programs\Startup\Teams.lnk",
                ),
            ],
        }
    }

    fn state(m: &MockStartup, name: &str) -> bool {
        is_enabled(&m.items.iter().find(|e| e.name == name).unwrap().approval)
    }

    #[test]
    fn approval_bytes_follow_task_manager() {
        assert!(is_enabled(&None));
        assert!(is_enabled(&Some(vec![2, 0, 0, 0])));
        assert!(is_enabled(&Some(vec![6, 0, 0, 0])));
        assert!(!is_enabled(&Some(vec![3, 0, 0, 0])));
        assert!(!is_enabled(&Some(vec![7, 0, 0, 0])));
        let d = disabled_value(0x0102_0304_0506_0708);
        assert_eq!(d.len(), 12);
        assert_eq!(&d[..4], &[3, 0, 0, 0]);
        assert_eq!(enabled_value().len(), 12);
    }

    #[test]
    fn recommended_disables_only_bloat_never_optional_or_useful() {
        let c = Catalog::builtin();
        let mut m = typical_pc();
        let mut j = StartupJournal::default();
        let r = apply_recommended(&mut m, &c, &mut j).unwrap();
        assert!(r.failed.is_empty(), "{:?}", r.failed);
        for off in [
            "MicrosoftEdgeAutoLaunch_0F1E2D",
            "Spotify",
            "AdobeGCInvoker-1.0",
            "Teams.lnk",
        ] {
            assert!(!state(&m, off), "{off} doit être désactivé");
        }
        for on in [
            "OneDrive",
            "Steam",
            "Discord",
            "SecurityHealth",
            "Riot Vanguard",
            "RtkAudUService",
        ] {
            assert!(state(&m, on), "{on} doit rester");
        }
        assert_eq!(j.originals.len(), 4);
    }

    #[test]
    fn protected_entries_are_refused_even_on_explicit_request() {
        let c = Catalog::builtin();
        let mut m = typical_pc();
        let mut j = StartupJournal::default();
        let mut r = StartupReport::default();
        for name in ["Riot Vanguard", "SecurityHealth"] {
            let e = m.items.iter().find(|e| e.name == name).unwrap().clone();
            disable(&mut m, &c, &e, &mut j, &mut r);
        }
        assert_eq!(r.failed.len(), 2);
        assert!(state(&m, "Riot Vanguard") && state(&m, "SecurityHealth"));
        assert!(j.originals.is_empty());
    }

    #[test]
    fn restore_puts_back_exactly_what_was_there() {
        let c = Catalog::builtin();
        let mut m = typical_pc();
        // Spotify était déjà désactivé par l'utilisateur ; Teams avait une valeur « activé » explicite.
        m.items.iter_mut().find(|e| e.name == "Spotify").unwrap().approval =
            Some(vec![3, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8]);
        m.items.iter_mut().find(|e| e.name == "Teams.lnk").unwrap().approval =
            Some(vec![2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        let before = m.items.clone();
        let mut j = StartupJournal::default();
        apply_recommended(&mut m, &c, &mut j).unwrap();
        let r = restore(&mut m, &mut j);
        assert!(r.failed.is_empty());
        assert_eq!(m.items, before);
        assert!(j.originals.is_empty());
    }

    #[test]
    fn enabling_then_restoring_returns_to_disabled() {
        let mut m = typical_pc();
        m.items[2].approval = Some(disabled_value(5));
        let before = m.items[2].clone();
        let mut j = StartupJournal::default();
        let mut r = StartupReport::default();
        enable(&mut m, &before, &mut j, &mut r);
        assert!(state(&m, "Spotify"));
        restore(&mut m, &mut j);
        assert_eq!(m.items[2], before);
    }

    #[test]
    fn catalog_rules_are_lowercase_and_protections_cover_anticheats() {
        let c = Catalog::builtin();
        let vg = entry(
            Source::MachineRun,
            "Riot Vanguard",
            r"C:\Program Files\Riot Vanguard\vgtray.exe",
        );
        assert!(c.protection(&vg).is_some());
        let eac = entry(
            Source::MachineRun,
            "x",
            r"C:\Games\EasyAntiCheat\EasyAntiCheat_launcher.exe",
        );
        assert!(c.protection(&eac).is_some());
        assert!(Catalog::parse(&DEMARRAGE_TOML.replace("\"*teams*\"", "\"*Teams*\"")).is_err());
    }
}
