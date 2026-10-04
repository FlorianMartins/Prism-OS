//! Plateforme simulée : sert aux tests du moteur et à `prism demo` hors Windows.
//!
//! Elle modélise ce qui compte pour la politique RAM, tel que mesuré sur Windows 11
//! (spec §4) : une page privée rognée finit dans le cache de priorité 0, et la purge
//! « basse » ne libère que ce cache-là ; le reste (cache des fichiers du jeu) reste.

use crate::journal::Undo;
use crate::model::{EcoState, MemPriority, MemStatus, Priority, ProcId, ProcInfo, PurgeScope, Snapshot, Target};
use crate::plan::Action;
use crate::platform::{Outcome, Platform};

/// Raison commune aux plateformes : on ne remonte jamais un processus.
pub const ALREADY_LOWER: &str = "déjà à ce niveau ou plus bas";

pub const BALANCED_PLAN: &str = "381b4222-f694-41f0-9685-ff5bb260df2e";
pub const HIGH_PERF_PLAN: &str = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";

#[derive(Clone, Debug)]
pub struct MockProc {
    pub info: ProcInfo,
    pub priority: Priority,
    pub eco: EcoState,
    pub mem_priority: MemPriority,
    /// Processus qu'on ne peut pas ouvrir (autre utilisateur, élevé…).
    pub denied: bool,
    /// CPU sets imposés (vide = tous les cœurs).
    pub cpu_sets: Vec<u32>,
    pub suspended: bool,
}

#[derive(Clone, Debug)]
pub struct MockPlatform {
    pub procs: Vec<MockProc>,
    pub total: u64,
    /// Cache en attente, par priorité mémoire (0..=7 comme Windows).
    pub standby: [u64; 8],
    pub power_plan: String,
    pub wsl_running: bool,
    pub user_session: u32,
    pub self_pid: u32,
    /// Journal lisible de tout ce que la plateforme a fait.
    pub log: Vec<String>,
    pub cpus: Vec<crate::model::CpuInfo>,
    /// Services connus : nom (minuscules) -> en cours d'exécution.
    pub services: std::collections::BTreeMap<String, bool>,
    pub foreground: Option<u32>,
}

impl MockPlatform {
    pub fn new(total: u64) -> MockPlatform {
        MockPlatform {
            procs: Vec::new(),
            total,
            standby: [0; 8],
            power_plan: BALANCED_PLAN.into(),
            wsl_running: false,
            user_session: 1,
            self_pid: 1,
            log: Vec::new(),
            cpus: Vec::new(),
            services: std::collections::BTreeMap::new(),
            foreground: None,
        }
    }

    /// Ajoute un processus de la session utilisateur et renvoie son identité.
    pub fn spawn(&mut self, name: &str, path: Option<&str>, working_set: u64) -> ProcId {
        let pid = 100 + self.procs.len() as u32 * 4;
        let id = ProcId {
            pid,
            created: 1_000_000 + pid as u64,
        };
        self.procs.push(MockProc {
            info: ProcInfo {
                id,
                name: name.to_lowercase(),
                path: path.map(|p| p.to_lowercase()),
                session: self.user_session,
                working_set,
                cpu_time: 0,
                parent: 0,
            },
            priority: Priority::Normal,
            eco: EcoState::SystemManaged,
            mem_priority: MemPriority::Normal,
            denied: false,
            cpu_sets: Vec::new(),
            suspended: false,
        });
        id
    }

    /// Le processus consomme `ms` millisecondes de processeur.
    pub fn work(&mut self, id: ProcId, ms: u64) {
        if let Some(p) = self.procs.iter_mut().find(|p| p.info.id == id) {
            p.info.cpu_time += ms * 10_000;
        }
    }

    pub fn kill(&mut self, id: ProcId) {
        self.procs.retain(|p| p.info.id != id);
    }

    /// Simule la réutilisation d'un PID : même PID, autre processus.
    pub fn reuse_pid(&mut self, old: ProcId, name: &str) -> ProcId {
        self.kill(old);
        let id = ProcId {
            pid: old.pid,
            created: old.created + 1,
        };
        self.procs.push(MockProc {
            info: ProcInfo {
                id,
                name: name.into(),
                path: None,
                session: self.user_session,
                working_set: 0,
                cpu_time: 0,
                parent: 0,
            },
            priority: Priority::Normal,
            eco: EcoState::SystemManaged,
            mem_priority: MemPriority::Normal,
            denied: false,
            cpu_sets: Vec::new(),
            suspended: false,
        });
        id
    }

    pub fn get(&self, id: ProcId) -> Option<&MockProc> {
        self.procs.iter().find(|p| p.info.id == id)
    }

    fn find(&mut self, target: &Target) -> Result<&mut MockProc, Outcome> {
        match self.procs.iter_mut().find(|p| p.info.id == target.id) {
            None => Err(Outcome::Skipped("processus disparu".into())),
            Some(p) if p.denied => Err(Outcome::Skipped("accès refusé".into())),
            Some(p) => Ok(p),
        }
    }

    fn used(&self) -> u64 {
        self.procs.iter().map(|p| p.info.working_set).sum::<u64>() + self.standby.iter().sum::<u64>()
    }

    pub fn mem(&self) -> MemStatus {
        MemStatus {
            total: self.total,
            free: self.total.saturating_sub(self.used()),
            standby_low: self.standby[0],
            standby_total: self.standby.iter().sum(),
        }
    }
}

impl Platform for MockPlatform {
    fn snapshot(&mut self) -> Result<Snapshot, String> {
        let mut procs: Vec<ProcInfo> = self.procs.iter().map(|p| p.info.clone()).collect();
        if self.wsl_running {
            procs.push(ProcInfo {
                id: ProcId {
                    pid: 9000,
                    created: 9000,
                },
                name: "vmmemwsl".into(),
                path: None,
                session: 0,
                working_set: 0,
                cpu_time: 0,
                parent: 0,
            });
        }
        Ok(Snapshot {
            procs,
            mem: self.mem(),
            user_session: self.user_session,
            self_pid: self.self_pid,
            cpus: self.cpus.clone(),
            foreground_pid: self.foreground,
            windowed: Vec::new(),
        })
    }

    fn apply(&mut self, action: &Action) -> Outcome {
        self.log.push(action.describe());
        match action {
            Action::Priority { target, to } => match self.find(target) {
                Err(o) => o,
                Ok(p) if p.priority.rank() <= to.rank() => Outcome::Skipped(ALREADY_LOWER.into()),
                Ok(p) => {
                    let previous = std::mem::replace(&mut p.priority, *to);
                    Outcome::Done(Some(Undo::Priority {
                        target: target.clone(),
                        previous,
                    }))
                }
            },
            Action::EcoQos { target } => match self.find(target) {
                Err(o) => o,
                Ok(p) => {
                    let previous = std::mem::replace(&mut p.eco, EcoState::On);
                    Outcome::Done(Some(Undo::EcoQos {
                        target: target.clone(),
                        previous,
                    }))
                }
            },
            Action::MemoryPriority { target, to } => match self.find(target) {
                Err(o) => o,
                Ok(p) if p.mem_priority <= *to => Outcome::Skipped(ALREADY_LOWER.into()),
                Ok(p) => {
                    let previous = std::mem::replace(&mut p.mem_priority, *to);
                    Outcome::Done(Some(Undo::MemoryPriority {
                        target: target.clone(),
                        previous,
                    }))
                }
            },
            Action::TrimWorkingSet { target } => match self.find(target) {
                Err(o) => o,
                Ok(p) => {
                    // Mesuré sur Windows 11 : une page privée rognée, une fois écrite
                    // dans le fichier d'échange, arrive en cache de priorité 0, quelle
                    // que soit la priorité mémoire du processus (1, 2 ou 5).
                    // Mesuré en VM : le temps noyau du rognage est imputé au processus
                    // rogné (environ 50 ms pour 1,5 Go).
                    p.info.cpu_time += 50 * 10_000;
                    let pages = std::mem::take(&mut p.info.working_set);
                    self.standby[0] += pages;
                    Outcome::Done(None)
                }
            },
            Action::PurgeStandby { scope } => {
                match scope {
                    PurgeScope::Off => {}
                    PurgeScope::Low => self.standby[0] = 0,
                    PurgeScope::All => self.standby = [0; 8],
                }
                Outcome::Done(None)
            }
            Action::HighPerformancePower => {
                let previous = std::mem::replace(&mut self.power_plan, HIGH_PERF_PLAN.into());
                Outcome::Done(Some(Undo::PowerPlan { previous }))
            }
            Action::ShutdownWsl => {
                self.wsl_running = false;
                Outcome::Done(None)
            }
            Action::CpuSets { target, cpus } => match self.find(target) {
                Err(o) => o,
                // Un choix déjà fait (par l'utilisateur ou un autre outil) est respecté.
                Ok(p) if !p.cpu_sets.is_empty() => Outcome::Skipped("cœurs déjà choisis".into()),
                Ok(p) => {
                    let previous = std::mem::replace(&mut p.cpu_sets, cpus.clone());
                    Outcome::Done(Some(Undo::CpuSets {
                        target: target.clone(),
                        previous,
                    }))
                }
            },
            Action::PauseService { name } => match self.services.get_mut(&name.to_lowercase()) {
                None => Outcome::Skipped("service absent".into()),
                Some(running) if !*running => Outcome::Skipped("déjà arrêté".into()),
                Some(running) => {
                    *running = false;
                    Outcome::Done(Some(Undo::Service { name: name.clone() }))
                }
            },
            Action::Suspend { target } => match self.find(target) {
                Err(o) => o,
                Ok(p) if p.suspended => Outcome::Skipped("déjà gelé".into()),
                Ok(p) => {
                    p.suspended = true;
                    Outcome::Done(Some(Undo::Resume { target: target.clone() }))
                }
            },
        }
    }

    fn undo(&mut self, undo: &Undo) -> Outcome {
        self.log.push(undo.describe());
        match undo {
            Undo::Priority { target, previous } => match self.find(target) {
                Err(o) => o,
                Ok(p) => {
                    p.priority = *previous;
                    Outcome::Done(None)
                }
            },
            Undo::EcoQos { target, previous } => match self.find(target) {
                Err(o) => o,
                Ok(p) => {
                    p.eco = *previous;
                    Outcome::Done(None)
                }
            },
            Undo::MemoryPriority { target, previous } => match self.find(target) {
                Err(o) => o,
                Ok(p) => {
                    p.mem_priority = *previous;
                    Outcome::Done(None)
                }
            },
            Undo::PowerPlan { previous } => {
                self.power_plan = previous.clone();
                Outcome::Done(None)
            }
            Undo::CpuSets { target, previous } => match self.find(target) {
                Err(o) => o,
                Ok(p) => {
                    p.cpu_sets = previous.clone();
                    Outcome::Done(None)
                }
            },
            Undo::Service { name } => {
                self.services.insert(name.to_lowercase(), true);
                Outcome::Done(None)
            }
            Undo::Resume { target } => match self.find(target) {
                Err(o) => o,
                Ok(p) => {
                    p.suspended = false;
                    Outcome::Done(None)
                }
            },
        }
    }
}
