//! Partie protégée par un anti-cheat noyau (Vanguard, EasyAntiCheat, BattlEye,
//! FACEIT, EA AntiCheat, Ricochet) : automatique, réglable élément par élément.
//!
//! À l'entrée (un jeu tourne et le processus d'un anti-cheat noyau est apparu) :
//! - les services du niveau Extrême retrouvent leur réglage d'origine ;
//! - les outils cyber passent en veille : programmes qui gênent les anti-cheats fermés
//!   (débogueurs, éditeurs de mémoire…), leurs services et pilotes arrêtés, WSL éteint.
//!
//! À la sortie (fin de la partie, ou arrêt de Prism) : les services Extrême sont
//! recoupés et les services d'outils arrêtés sont relancés. Les programmes fermés ne
//! sont pas rouverts (leur état est perdu de toute façon).
//!
//! L'état est écrit sur disque : après un arrêt brutal, la sortie est rejouée.
//! Rien ici ne touche au jeu ni à l'anti-cheat lui-même (docs/anticheat-rules.md).

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::allege::{self, AllegeJournal, Catalog, Change, Original, StartType, SystemConfig, Tier};
use crate::config::{Config, GamingConflict};
use crate::glob::any_matches;
use crate::model::{ProcId, Snapshot};

/// Processus des anti-cheats noyau, et le nom montré à l'utilisateur. Ce sont leurs
/// services (démarrés avec le jeu) ou, pour Ricochet qui n'a pas de processus à lui,
/// le jeu lui-même.
pub const ANTICHEATS: [(&str, &str); 9] = [
    ("vgc.exe", "Vanguard (Riot)"),
    ("easyanticheat.exe", "EasyAntiCheat"),
    ("easyanticheat_eos.exe", "EasyAntiCheat"),
    ("beservice.exe", "BattlEye"),
    ("beservice_x64.exe", "BattlEye"),
    ("faceitservice.exe", "FACEIT"),
    ("faceit.exe", "FACEIT"),
    ("eaanticheat.gameservice.exe", "EA AntiCheat"),
    ("cod.exe", "Ricochet (Call of Duty)"),
];

/// Anti-cheat noyau en cours d'exécution, s'il y en a un.
pub fn anticheat(snap: &Snapshot) -> Option<&'static str> {
    snap.procs
        .iter()
        .find_map(|p| ANTICHEATS.iter().find(|(n, _)| p.name == *n).map(|(_, label)| *label))
}

/// Réglages (tout est actif par défaut ; l'utilisateur coupe ce qu'il veut garder).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Reglages {
    /// Le plan entier.
    pub actif: bool,
    /// Remettre les services du niveau Extrême pendant la partie.
    pub services_extreme: bool,
    /// Fermer les programmes qui gênent les anti-cheats (débogueurs, éditeurs de mémoire).
    pub fermer_outils_genants: bool,
    /// Fermer aussi les autres outils (Wireshark, machines virtuelles…). Désactivé par
    /// défaut : une machine virtuelle fermée perd son travail en cours.
    pub fermer_tous_les_outils: bool,
    /// Arrêter les services et pilotes des outils (Npcap, VMware, pilotes Sysinternals…).
    pub arreter_services_outils: bool,
    /// Éteindre WSL (Kali) : rend la RAM de sa machine virtuelle.
    pub eteindre_wsl: bool,
    /// Outils (identifiants du catalogue) jamais touchés.
    pub exclus: Vec<String>,
}

impl Default for Reglages {
    fn default() -> Self {
        Reglages {
            actif: true,
            services_extreme: true,
            fermer_outils_genants: true,
            fermer_tous_les_outils: false,
            arreter_services_outils: true,
            eteindre_wsl: true,
            exclus: Vec::new(),
        }
    }
}

impl Reglages {
    pub const FICHIER: &'static str = "jeu-noyau.json";

    pub fn charger(dir: &Path) -> Reglages {
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

/// Ce qui a été fait à l'entrée, pour la sortie.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Etat {
    pub anticheat: String,
    /// Services Extrême remis : à recouper à la sortie (nom, réglage du catalogue).
    pub services_remis: Vec<(String, StartType)>,
    /// Services d'outils arrêtés : à relancer à la sortie.
    pub services_arretes: Vec<String>,
}

impl Etat {
    pub const FICHIER: &'static str = "jeu-noyau-etat.json";

    pub fn charger(dir: &Path) -> Option<Etat> {
        std::fs::read(dir.join(Self::FICHIER))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
    }

    pub fn enregistrer(&self, dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let tmp = dir.join("jeu-noyau-etat.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, dir.join(Self::FICHIER)).map_err(|e| e.to_string())
    }

    pub fn effacer(dir: &Path) {
        let _ = std::fs::remove_file(dir.join(Self::FICHIER));
    }
}

/// Mise en veille des outils (Windows, ou simulation).
pub trait Veille {
    /// `Ok(None)` : service absent de cette machine.
    fn service_en_marche(&mut self, name: &str) -> Result<Option<bool>, String>;
    fn arreter_service(&mut self, name: &str) -> Result<(), String>;
    fn demarrer_service(&mut self, name: &str) -> Result<(), String>;
    /// Ferme le processus s'il a toujours cette identité (PID + date de création).
    fn fermer(&mut self, id: &ProcId) -> Result<(), String>;
    fn eteindre_wsl(&mut self) -> Result<(), String>;
}

/// Services Extrême du catalogue (noms en minuscules) et leur réglage cible.
fn services_extreme(catalog: &Catalog) -> Vec<(String, StartType)> {
    catalog
        .services
        .iter()
        .filter(|s| s.tier == Tier::Extreme)
        .map(|s| (s.name.clone(), s.start))
        .collect()
}

/// Entrée dans une partie protégée. `log` reçoit chaque action, en clair.
#[allow(clippy::too_many_arguments)]
pub fn entrer(
    sys: &mut dyn SystemConfig,
    veille: &mut dyn Veille,
    catalog: &Catalog,
    journal: &mut AllegeJournal,
    cfg: &Config,
    snap: &Snapshot,
    reglages: &Reglages,
    anticheat: &str,
    log: &mut Vec<String>,
) -> Etat {
    let mut etat = Etat {
        anticheat: anticheat.to_string(),
        ..Default::default()
    };
    if !reglages.actif {
        return etat;
    }

    // 1. Services Extrême : ceux que Prism a changés (présents au journal) reviennent
    //    à leur réglage d'origine, et sortent du journal le temps de la partie.
    if reglages.services_extreme {
        let cibles = services_extreme(catalog);
        let (remis, gardes): (Vec<Original>, Vec<Original>) = journal.originals.drain(..).partition(|o| match o {
            Original::ServiceStart { name, .. } => cibles.iter().any(|(n, _)| n.eq_ignore_ascii_case(name)),
            _ => false,
        });
        journal.originals = gardes;
        let mut partiel = AllegeJournal {
            originals: remis.clone(),
        };
        let r = allege::restore(sys, &mut partiel);
        log.extend(r.done.iter().map(|d| format!("remis pour la partie : {d}")));
        log.extend(r.failed.iter().map(|f| format!("ÉCHEC : {f}")));
        // Ce qui n'a pas pu être remis reste au journal (rien n'est perdu).
        journal.originals.extend(partiel.originals.iter().cloned());
        for o in remis.iter().filter(|o| !partiel.originals.contains(o)) {
            if let Original::ServiceStart { name, .. } = o {
                if let Some((_, to)) = cibles.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)) {
                    etat.services_remis.push((name.clone(), *to));
                }
            }
        }
    }

    // 2. Outils : programmes fermés, puis services arrêtés (un service n'est arrêté que
    //    si plus aucun programme de l'outil ne tourne).
    for tool in cfg.tools.iter().filter(|t| !reglages.exclus.contains(&t.id)) {
        let genant = tool.gaming_conflict == GamingConflict::Warn;
        let fermer = (genant && reglages.fermer_outils_genants) || reglages.fermer_tous_les_outils;
        let procs: Vec<_> = snap
            .procs
            .iter()
            .filter(|p| p.session == snap.user_session && any_matches(&tool.processes, &p.name))
            .collect();
        let mut encore = false;
        for p in &procs {
            if !fermer {
                encore = true;
                continue;
            }
            match veille.fermer(&p.id) {
                Ok(()) => log.push(format!("{} : {} fermé", tool.name, p.name)),
                Err(e) => {
                    encore = true;
                    log.push(format!("ÉCHEC : {} : {} non fermé ({e})", tool.name, p.name));
                }
            }
        }
        if !reglages.arreter_services_outils || encore {
            continue;
        }
        for svc in &tool.services {
            match veille.service_en_marche(svc) {
                Ok(Some(true)) => match veille.arreter_service(svc) {
                    Ok(()) => {
                        log.push(format!("{} : service {svc} arrêté", tool.name));
                        etat.services_arretes.push(svc.clone());
                    }
                    Err(e) => log.push(format!("ÉCHEC : {} : service {svc} ({e})", tool.name)),
                },
                Ok(_) => {}
                Err(e) => log.push(format!("ÉCHEC : {} : service {svc} illisible ({e})", tool.name)),
            }
        }
    }

    // 3. WSL : sa machine virtuelle garde plusieurs Go de RAM.
    if reglages.eteindre_wsl
        && snap
            .procs
            .iter()
            .any(|p| p.name == "vmmemwsl" || p.name == "wslservice.exe")
    {
        match veille.eteindre_wsl() {
            Ok(()) => log.push("WSL éteint".into()),
            Err(e) => log.push(format!("ÉCHEC : WSL ({e})")),
        }
    }
    etat
}

/// Sortie : services Extrême recoupés (leur valeur d'origine revient au journal) et
/// services d'outils relancés.
pub fn sortir(
    sys: &mut dyn SystemConfig,
    veille: &mut dyn Veille,
    catalog: &Catalog,
    journal: &mut AllegeJournal,
    etat: &Etat,
    save: &mut dyn FnMut(&AllegeJournal) -> Result<(), String>,
    log: &mut Vec<String>,
) {
    let changes: Vec<Change> = etat
        .services_remis
        .iter()
        .map(|(name, to)| Change::ServiceStart {
            name: name.clone(),
            to: *to,
        })
        .collect();
    let r = allege::apply(sys, catalog, &changes, journal, save);
    log.extend(r.done.iter().map(|d| format!("fin de partie : {d}")));
    log.extend(r.failed.iter().map(|f| format!("ÉCHEC : {f}")));
    for svc in &etat.services_arretes {
        match veille.demarrer_service(svc) {
            Ok(()) => log.push(format!("service {svc} relancé")),
            Err(e) => log.push(format!("ÉCHEC : service {svc} non relancé ({e})")),
        }
    }
}

/// Simulation pour les tests.
#[derive(Debug, Default)]
pub struct MockVeille {
    /// Service -> en marche.
    pub services: std::collections::BTreeMap<String, bool>,
    pub fermes: Vec<u32>,
    pub wsl_eteint: bool,
}

impl Veille for MockVeille {
    fn service_en_marche(&mut self, name: &str) -> Result<Option<bool>, String> {
        Ok(self.services.get(name).copied())
    }
    fn arreter_service(&mut self, name: &str) -> Result<(), String> {
        self.services.insert(name.into(), false);
        Ok(())
    }
    fn demarrer_service(&mut self, name: &str) -> Result<(), String> {
        self.services.insert(name.into(), true);
        Ok(())
    }
    fn fermer(&mut self, id: &ProcId) -> Result<(), String> {
        self.fermes.push(id.pid);
        Ok(())
    }
    fn eteindre_wsl(&mut self) -> Result<(), String> {
        self.wsl_eteint = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::allege::{plan, MockSystem};
    use crate::classify::tests::proc;

    fn snap(procs: Vec<crate::model::ProcInfo>) -> Snapshot {
        Snapshot {
            procs,
            mem: Default::default(),
            user_session: 1,
            self_pid: 1,
            cpus: Vec::new(),
            foreground_pid: None,
        }
    }

    fn cfg() -> Config {
        Config::parse(crate::config::DEFAULT_TOML).unwrap()
    }

    /// Windows d'usine : chaque service Extrême en automatique, puis niveau Extrême appliqué.
    fn extreme_applique(c: &Catalog) -> (MockSystem, AllegeJournal) {
        let mut m = MockSystem::default();
        for s in &c.services {
            m.services.insert(s.name.to_ascii_lowercase(), StartType::Auto);
        }
        let mut j = AllegeJournal::default();
        allege::apply(
            &mut m,
            c,
            &plan(c, &[Tier::Sur, Tier::Extreme]),
            &mut j,
            &mut |_| Ok(()),
        );
        (m, j)
    }

    #[test]
    fn detects_kernel_anticheats_only() {
        assert_eq!(
            anticheat(&snap(vec![proc(10, "vgc.exe", None)])),
            Some("Vanguard (Riot)")
        );
        assert_eq!(
            anticheat(&snap(vec![proc(10, "beservice_x64.exe", None)])),
            Some("BattlEye")
        );
        assert_eq!(anticheat(&snap(vec![proc(10, "cs2.exe", None)])), None);
    }

    #[test]
    fn extreme_services_come_back_for_the_game_and_leave_after() {
        let c = Catalog::builtin();
        let (mut m, mut j) = extreme_applique(&c);
        assert_eq!(m.services["dps"], StartType::Manual);
        let sur_avant = j
            .originals
            .iter()
            .filter(|o| matches!(o, Original::ServiceStart { name, .. } if name == "DiagTrack"))
            .count();
        let mut v = MockVeille::default();
        let mut log = Vec::new();
        let etat = entrer(
            &mut m,
            &mut v,
            &c,
            &mut j,
            &cfg(),
            &snap(vec![]),
            &Reglages::default(),
            "Vanguard (Riot)",
            &mut log,
        );
        // Extrême remis, le niveau sûr (télémétrie) reste coupé.
        assert_eq!(m.services["dps"], StartType::Auto);
        assert_eq!(m.services["diagtrack"], StartType::Disabled);
        assert!(!etat.services_remis.is_empty());
        assert!(j
            .originals
            .iter()
            .all(|o| !matches!(o, Original::ServiceStart { name, .. } if name == "DPS")));
        sortir(&mut m, &mut v, &c, &mut j, &etat, &mut |_| Ok(()), &mut log);
        assert_eq!(m.services["dps"], StartType::Manual);
        // L'origine (automatique) est de nouveau au journal : « restore » la remettra.
        assert!(j
            .originals
            .iter()
            .any(|o| matches!(o, Original::ServiceStart { name, was: StartType::Auto } if name == "DPS")));
        let sur_apres = j
            .originals
            .iter()
            .filter(|o| matches!(o, Original::ServiceStart { name, .. } if name == "DiagTrack"))
            .count();
        assert_eq!(sur_avant, sur_apres);
    }

    #[test]
    fn nothing_happens_when_the_plan_is_off() {
        let c = Catalog::builtin();
        let (mut m, mut j) = extreme_applique(&c);
        let avant = j.clone();
        let mut v = MockVeille::default();
        let r = Reglages {
            actif: false,
            ..Default::default()
        };
        let etat = entrer(
            &mut m,
            &mut v,
            &c,
            &mut j,
            &cfg(),
            &snap(vec![proc(20, "x64dbg.exe", None)]),
            &r,
            "BattlEye",
            &mut Vec::new(),
        );
        assert_eq!(j, avant);
        assert!(v.fermes.is_empty() && etat.services_remis.is_empty());
    }

    #[test]
    fn interfering_tools_are_closed_others_kept_unless_asked() {
        let c = Catalog::builtin();
        let mut m = MockSystem::default();
        let mut j = AllegeJournal::default();
        let mut v = MockVeille::default();
        v.services.insert("npcap".into(), true);
        v.services.insert("VMAuthdService".into(), true);
        let s = snap(vec![
            proc(20, "x64dbg.exe", None),
            proc(21, "cheatengine-x86_64.exe", None),
            proc(22, "vmware.exe", None),
        ]);
        let etat = entrer(
            &mut m,
            &mut v,
            &c,
            &mut j,
            &cfg(),
            &s,
            &Reglages::default(),
            "EasyAntiCheat",
            &mut Vec::new(),
        );
        assert_eq!(v.fermes, vec![20, 21]);
        // Wireshark ne tourne pas : Npcap arrêté. VMware tourne : ses services restent.
        assert_eq!(etat.services_arretes, vec!["npcap".to_string()]);
        assert!(v.services["VMAuthdService"]);
        sortir(&mut m, &mut v, &c, &mut j, &etat, &mut |_| Ok(()), &mut Vec::new());
        assert!(v.services["npcap"]);
    }

    #[test]
    fn excluded_tools_are_never_touched_and_wsl_is_shut_down() {
        let c = Catalog::builtin();
        let mut v = MockVeille::default();
        let r = Reglages {
            exclus: vec!["x64dbg".into()],
            ..Default::default()
        };
        let s = snap(vec![proc(20, "x64dbg.exe", None), proc(30, "vmmemwsl", None)]);
        entrer(
            &mut MockSystem::default(),
            &mut v,
            &c,
            &mut AllegeJournal::default(),
            &cfg(),
            &s,
            &r,
            "FACEIT",
            &mut Vec::new(),
        );
        assert!(v.fermes.is_empty());
        assert!(v.wsl_eteint);
    }

    #[test]
    fn settings_default_to_automatic_and_survive_a_partial_file() {
        let dir = std::env::temp_dir().join(format!("prism-noyau-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(Reglages::FICHIER), r#"{"eteindre_wsl": false}"#).unwrap();
        let r = Reglages::charger(&dir);
        assert!(r.actif && r.services_extreme && !r.eteindre_wsl);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
