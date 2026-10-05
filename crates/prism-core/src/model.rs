//! Types partagés entre le cœur et les plateformes.

use serde::{Deserialize, Serialize};

/// Identité d'un processus : le PID seul ne suffit pas, Windows le réutilise.
/// `created` est la date de création (FILETIME sous Windows).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProcId {
    pub pid: u32,
    pub created: u64,
}

/// Un processus tel que vu au moment du relevé.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcInfo {
    pub id: ProcId,
    /// Nom de l'exécutable, en minuscules (`word.exe`).
    pub name: String,
    /// Chemin complet en minuscules avec des `\`, si lisible.
    pub path: Option<String>,
    pub session: u32,
    /// Mémoire de travail en octets (0 si illisible).
    pub working_set: u64,
    /// Temps processeur cumulé (noyau + utilisateur), en unités de 100 ns.
    pub cpu_time: u64,
    /// PID du processus parent (0 si inconnu). Peut avoir été réutilisé : comparer les
    /// dates de création avant de s'y fier.
    pub parent: u32,
}

/// Un processeur logique tel que Windows le décrit (CPU set).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CpuInfo {
    /// Identifiant de CPU set (ce que prend `SetProcessDefaultCpuSets`).
    pub id: u32,
    /// Plus elle est haute, plus le cœur est performant (P > E).
    pub efficiency_class: u8,
    /// Taille du cache de dernier niveau partagé par ce cœur (0 si inconnue).
    pub llc_bytes: u64,
}

/// État de la mémoire physique, en octets.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemStatus {
    pub total: u64,
    /// Pages libres + pages mises à zéro : utilisables sans rien reprendre.
    pub free: u64,
    /// Cache en attente de priorité 0 : la seule partie que Windows sait purger sans
    /// toucher au reste du cache. Les pages privées rognées puis écrites dans le
    /// fichier d'échange y arrivent (mesuré, voir spec §4).
    pub standby_low: u64,
    /// Tout le cache en attente.
    pub standby_total: u64,
}

impl MemStatus {
    /// Mémoire utilisée, comme « En cours d'utilisation » du Gestionnaire des tâches :
    /// ni libre ni cache en attente (que Windows reprend à la demande).
    pub fn used(&self) -> u64 {
        self.total.saturating_sub(self.free.saturating_add(self.standby_total))
    }

    pub fn free_percent(&self) -> u64 {
        if self.total == 0 {
            return 100;
        }
        self.free.saturating_mul(100) / self.total
    }
}

/// Relevé complet de la machine à un instant.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub procs: Vec<ProcInfo>,
    pub mem: MemStatus,
    /// Session interactive de l'utilisateur : seuls ses processus sont concernés.
    pub user_session: u32,
    pub self_pid: u32,
    /// Topologie du processeur (vide si inconnue).
    pub cpus: Vec<CpuInfo>,
    /// Processus de la fenêtre au premier plan (celle que l'utilisateur regarde).
    pub foreground_pid: Option<u32>,
    /// Processus qui ont au moins une fenêtre visible (ni cachée, ni outil, ni masquée).
    pub windowed: Vec<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Idle,
    BelowNormal,
    Normal,
    AboveNormal,
    High,
    Realtime,
}

impl Priority {
    /// Ordre croissant : sert à ne jamais *remonter* un processus déjà plus bas.
    pub fn rank(self) -> u8 {
        match self {
            Priority::Idle => 0,
            Priority::BelowNormal => 1,
            Priority::Normal => 2,
            Priority::AboveNormal => 3,
            Priority::High => 4,
            Priority::Realtime => 5,
        }
    }
}

/// Priorité mémoire Windows (`MEMORY_PRIORITY_*`) : plus elle est basse, plus les
/// pages du processus sont reprises tôt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemPriority {
    VeryLow,
    Low,
    Medium,
    BelowNormal,
    Normal,
}

impl MemPriority {
    pub fn level(self) -> u32 {
        match self {
            MemPriority::VeryLow => 1,
            MemPriority::Low => 2,
            MemPriority::Medium => 3,
            MemPriority::BelowNormal => 4,
            MemPriority::Normal => 5,
        }
    }

    pub fn from_level(level: u32) -> MemPriority {
        match level {
            0 | 1 => MemPriority::VeryLow,
            2 => MemPriority::Low,
            3 => MemPriority::Medium,
            4 => MemPriority::BelowNormal,
            _ => MemPriority::Normal,
        }
    }
}

/// État EcoQoS d'un processus avant qu'on y touche.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EcoState {
    /// Windows décide seul (cas de presque tous les processus).
    SystemManaged,
    On,
    Off,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PurgeScope {
    Off,
    /// Seulement le cache en attente de priorité basse.
    Low,
    /// Tout le cache en attente (sur demande explicite).
    All,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PowerPlan {
    Unchanged,
    HighPerformance,
}

/// Processus visé par une action, avec son nom pour les journaux lisibles.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Target {
    pub id: ProcId,
    pub name: String,
}

impl Target {
    pub fn of(p: &ProcInfo) -> Target {
        Target {
            id: p.id,
            name: p.name.clone(),
        }
    }
}

/// Format court lisible : 1536 Mo -> "1.5 Go".
pub fn human_bytes(b: u64) -> String {
    const MIB: f64 = 1024.0 * 1024.0;
    let mib = b as f64 / MIB;
    if mib >= 1024.0 {
        format!("{:.1} Go", mib / 1024.0)
    } else {
        format!("{:.0} Mo", mib)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_percent_handles_zero_total() {
        assert_eq!(MemStatus::default().free_percent(), 100);
        let m = MemStatus {
            total: 1000,
            free: 250,
            ..Default::default()
        };
        assert_eq!(m.free_percent(), 25);
    }

    #[test]
    fn used_memory_excludes_free_pages_and_standby_cache() {
        let m = MemStatus {
            total: 32,
            free: 2,
            standby_low: 4,
            standby_total: 20,
        };
        assert_eq!(m.used(), 10);
        assert_eq!(MemStatus::default().used(), 0);
    }

    #[test]
    fn mem_priority_round_trips() {
        for p in [
            MemPriority::VeryLow,
            MemPriority::Low,
            MemPriority::Medium,
            MemPriority::BelowNormal,
            MemPriority::Normal,
        ] {
            assert_eq!(MemPriority::from_level(p.level()), p);
        }
    }

    #[test]
    fn human_bytes_units() {
        assert_eq!(human_bytes(512 * 1024 * 1024), "512 Mo");
        assert_eq!(human_bytes(1536 * 1024 * 1024), "1.5 Go");
    }
}
