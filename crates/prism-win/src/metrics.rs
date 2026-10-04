//! Mesures système pour la barre et les widgets : processeur, mémoire, réseau, GPU.

use std::ptr::null_mut;

use windows_sys::Win32::Foundation::FILETIME;
use windows_sys::Win32::NetworkManagement::IpHelper::{FreeMibTable, GetIfTable2, MIB_IF_TABLE2};
use windows_sys::Win32::System::Performance::{
    PdhAddEnglishCounterW, PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhGetFormattedCounterValue,
    PdhOpenQueryW, PDH_FMT_COUNTERVALUE, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE,
};
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows_sys::Win32::System::Threading::GetSystemTimes;

fn ft(f: FILETIME) -> u64 {
    ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64
}

const IF_TYPE_SOFTWARE_LOOPBACK: u32 = 24;

/// Handle de requête ou de compteur PDH.
type Pdh = *mut std::ffi::c_void;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sample {
    pub cpu: f32,
    pub ram: f32,
    pub ram_used: u64,
    pub ram_total: u64,
    /// Octets par seconde reçus / envoyés.
    pub net_down: f64,
    pub net_up: f64,
    /// `None` : pas de compteur GPU sur cette machine (VM, pilote de base).
    pub gpu: Option<f32>,
}

pub struct Metrics {
    last_cpu: Option<(u64, u64)>,
    last_net: Option<(u64, u64, std::time::Instant)>,
    gpu_query: Option<(Pdh, Pdh)>,
    /// Le compteur GPU (le plus coûteux) n'est lu qu'une mesure sur deux.
    gpu_turn: u32,
    last_gpu: Option<f32>,
    /// « % Processor Utility » : le chiffre du Gestionnaire des tâches de Windows 11
    /// (tient compte de la fréquence). Sans lui, le temps processeur classique.
    cpu_query: Option<(Pdh, Pdh)>,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    pub fn new() -> Metrics {
        Metrics {
            last_cpu: None,
            last_net: None,
            gpu_query: open_gpu_query(),
            gpu_turn: 0,
            last_gpu: None,
            cpu_query: open_query(r"\Processor Information(_Total)\% Processor Utility"),
        }
    }

    pub fn sample(&mut self) -> Sample {
        let mut s = Sample::default();
        // Processeur : part du temps non inactif depuis la mesure précédente.
        // SAFETY: sorties locales.
        unsafe {
            let (mut idle, mut kernel, mut user): (FILETIME, FILETIME, FILETIME) = std::mem::zeroed();
            if GetSystemTimes(&mut idle, &mut kernel, &mut user) != 0 {
                let busy_total = ft(kernel) + ft(user); // le temps noyau inclut l'inactivité
                let idle = ft(idle);
                if let Some((pi, pt)) = self.last_cpu {
                    let dt = busy_total.saturating_sub(pt);
                    if dt > 0 {
                        s.cpu = (1.0 - idle.saturating_sub(pi) as f64 / dt as f64) as f32 * 100.0;
                    }
                }
                self.last_cpu = Some((idle, busy_total));
            }
        }
        // Même mesure que le Gestionnaire des tâches quand le compteur existe.
        if let Some(u) = self.cpu_query.and_then(|(q, c)| single_value(q, c)) {
            s.cpu = u.clamp(0.0, 100.0) as f32;
        }
        // Mémoire.
        // SAFETY: structure de sortie locale, taille annoncée.
        unsafe {
            let mut ms: MEMORYSTATUSEX = std::mem::zeroed();
            ms.dwLength = size_of::<MEMORYSTATUSEX>() as u32;
            if GlobalMemoryStatusEx(&mut ms) != 0 {
                s.ram = ms.dwMemoryLoad as f32;
                s.ram_total = ms.ullTotalPhys;
                s.ram_used = ms.ullTotalPhys - ms.ullAvailPhys;
            }
        }
        // Réseau : somme des interfaces physiques, en débit.
        if let Some((down, up)) = net_octets() {
            let now = std::time::Instant::now();
            if let Some((pd, pu, t)) = self.last_net {
                let secs = now.duration_since(t).as_secs_f64().max(0.001);
                s.net_down = down.saturating_sub(pd) as f64 / secs;
                s.net_up = up.saturating_sub(pu) as f64 / secs;
            }
            self.last_net = Some((down, up, now));
        }
        if self.gpu_turn % 2 == 0 {
            self.last_gpu = self.gpu_query.and_then(|(q, c)| gpu_percent(q, c));
        }
        self.gpu_turn = self.gpu_turn.wrapping_add(1);
        s.gpu = self.last_gpu;
        s
    }
}

fn net_octets() -> Option<(u64, u64)> {
    // SAFETY: Windows alloue la table ; on la libère avec FreeMibTable ; on lit
    // `NumEntries` lignes à partir du premier élément.
    unsafe {
        let mut table: *mut MIB_IF_TABLE2 = null_mut();
        if GetIfTable2(&mut table) != 0 || table.is_null() {
            return None;
        }
        let n = (*table).NumEntries as usize;
        let rows = std::slice::from_raw_parts((*table).Table.as_ptr(), n);
        let (mut down, mut up) = (0u64, 0u64);
        for r in rows {
            // Interfaces matérielles seulement, sans les « filtres » de Windows (WFP, QoS…)
            // qui doublent chaque carte réseau et faisaient compter le trafic plusieurs fois.
            let flags = r.InterfaceAndOperStatusFlags._bitfield;
            let hardware = flags & 1 != 0;
            let filter = flags & 2 != 0;
            if r.Type != IF_TYPE_SOFTWARE_LOOPBACK && hardware && !filter {
                down += r.InOctets;
                up += r.OutOctets;
            }
        }
        FreeMibTable(table as *const _);
        Some((down, up))
    }
}

fn open_query(path: &str) -> Option<(Pdh, Pdh)> {
    // SAFETY: requête et compteur PDH détenus pour la vie du processus.
    unsafe {
        let mut q: Pdh = null_mut();
        if PdhOpenQueryW(std::ptr::null(), 0, &mut q) != 0 {
            return None;
        }
        let path: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
        let mut c: Pdh = null_mut();
        if PdhAddEnglishCounterW(q, path.as_ptr(), 0, &mut c) != 0 {
            return None;
        }
        PdhCollectQueryData(q);
        Some((q, c))
    }
}

/// Tous les moteurs de toutes les cartes (3D, vidéo, calcul, copie).
fn open_gpu_query() -> Option<(Pdh, Pdh)> {
    open_query(r"\GPU Engine(*)\Utilization Percentage")
}

fn single_value(q: Pdh, c: Pdh) -> Option<f64> {
    // SAFETY: structure de sortie locale.
    unsafe {
        if PdhCollectQueryData(q) != 0 {
            return None;
        }
        let mut v: PDH_FMT_COUNTERVALUE = std::mem::zeroed();
        if PdhGetFormattedCounterValue(c, PDH_FMT_DOUBLE, null_mut(), &mut v) != 0 {
            return None;
        }
        Some(v.Anonymous.doubleValue)
    }
}

fn gpu_percent(q: Pdh, c: Pdh) -> Option<f32> {
    // SAFETY: double appel PDH (taille puis données) dans un tampon aligné local.
    unsafe {
        if PdhCollectQueryData(q) != 0 {
            return None;
        }
        let (mut size, mut count) = (0u32, 0u32);
        PdhGetFormattedCounterArrayW(c, PDH_FMT_DOUBLE, &mut size, &mut count, null_mut());
        if size == 0 {
            return None;
        }
        let mut buf = vec![0u64; (size as usize).div_ceil(8)];
        let items = buf.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
        if PdhGetFormattedCounterArrayW(c, PDH_FMT_DOUBLE, &mut size, &mut count, items) != 0 {
            return None;
        }
        let samples: Vec<(String, f64)> = std::slice::from_raw_parts(items, count as usize)
            .iter()
            .map(|i| {
                let p = i.szName;
                let len = if p.is_null() {
                    0
                } else {
                    (0..).take_while(|k| *p.add(*k) != 0).count()
                };
                let name = if len == 0 {
                    String::new()
                } else {
                    String::from_utf16_lossy(std::slice::from_raw_parts(p, len))
                };
                (name, i.FmtValue.Anonymous.doubleValue)
            })
            .collect();
        Some(prism_core::mesures::gpu_busiest(&samples) as f32)
    }
}

/// « 1,2 Mo/s », « 340 Ko/s ».
pub fn human_rate(bytes_per_sec: f64) -> String {
    if bytes_per_sec >= 1024.0 * 1024.0 {
        format!("{:.1} Mo/s", bytes_per_sec / 1024.0 / 1024.0)
    } else {
        format!("{:.0} Ko/s", bytes_per_sec / 1024.0)
    }
}
