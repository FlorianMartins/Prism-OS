//! Répartition des cœurs : sur quels cœurs reléguer l'arrière-plan pour laisser les
//! meilleurs au jeu, **sans toucher au processus du jeu** (règle anti-cheat n° 1).
//!
//! Deux cas où le processeur n'est pas homogène :
//! - **hybride** (Intel 12e génération et suivantes) : cœurs performance (P) et cœurs
//!   économes (E), distingués par la classe d'efficacité que donne Windows ;
//! - **Ryzen X3D à deux puces** (7950X3D, 9950X3D…) : une seule puce porte le cache 3D,
//!   reconnaissable à un cache de dernier niveau (L3) plus grand.
//!
//! Partout ailleurs (processeur homogène, VM), Prism ne touche pas aux cœurs.

use crate::model::CpuInfo;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitKind {
    Hybrid,
    VCache,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreSplit {
    pub kind: SplitKind,
    /// Identifiants des CPU sets où placer l'arrière-plan.
    pub background: Vec<u32>,
    /// Ceux qui restent libres pour le jeu.
    pub game: Vec<u32>,
}

impl CoreSplit {
    pub fn describe(&self) -> String {
        match self.kind {
            SplitKind::Hybrid => format!(
                "processeur hybride : arrière-plan sur {} cœurs économes, {} cœurs performance laissés au jeu",
                self.background.len(),
                self.game.len()
            ),
            SplitKind::VCache => format!(
                "Ryzen X3D : arrière-plan sur la puce sans cache 3D ({} cœurs logiques), {} laissés au jeu",
                self.background.len(),
                self.game.len()
            ),
        }
    }
}

/// En dessous, reléguer l'arrière-plan sur une petite partie du processeur risque de
/// l'étouffer pour un gain nul.
const MIN_LOGICAL: usize = 8;

pub fn split(cpus: &[CpuInfo]) -> Option<CoreSplit> {
    if cpus.len() < MIN_LOGICAL {
        return None;
    }
    let ids = |pred: &dyn Fn(&CpuInfo) -> bool| -> Vec<u32> { cpus.iter().filter(|c| pred(c)).map(|c| c.id).collect() };

    let min_eff = cpus.iter().map(|c| c.efficiency_class).min()?;
    let max_eff = cpus.iter().map(|c| c.efficiency_class).max()?;
    if min_eff != max_eff {
        let background = ids(&|c| c.efficiency_class == min_eff);
        let game = ids(&|c| c.efficiency_class != min_eff);
        return Some(CoreSplit {
            kind: SplitKind::Hybrid,
            background,
            game,
        });
    }

    let max_llc = cpus.iter().map(|c| c.llc_bytes).max()?;
    let min_llc = cpus.iter().map(|c| c.llc_bytes).min()?;
    if min_llc > 0 && min_llc != max_llc {
        let background = ids(&|c| c.llc_bytes != max_llc);
        let game = ids(&|c| c.llc_bytes == max_llc);
        return Some(CoreSplit {
            kind: SplitKind::VCache,
            background,
            game,
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cpu(id: u32, eff: u8, llc_mb: u64) -> CpuInfo {
        CpuInfo {
            id,
            efficiency_class: eff,
            llc_bytes: llc_mb << 20,
        }
    }

    #[test]
    fn intel_hybrid_sends_background_to_e_cores() {
        // i7-13700K : 8 P (16 logiques, classe 1) + 8 E (classe 0).
        let mut cpus: Vec<CpuInfo> = (0..16).map(|i| cpu(256 + i, 1, 30)).collect();
        cpus.extend((16..24).map(|i| cpu(256 + i, 0, 30)));
        let s = split(&cpus).unwrap();
        assert_eq!(s.kind, SplitKind::Hybrid);
        assert_eq!(s.background.len(), 8);
        assert_eq!(s.game.len(), 16);
        assert!(s.background.iter().all(|id| *id >= 256 + 16));
    }

    #[test]
    fn dual_ccd_x3d_sends_background_to_the_ccd_without_vcache() {
        // 7950X3D : CCD0 96 Mo de L3 (cache 3D), CCD1 32 Mo ; 16 logiques chacun.
        let mut cpus: Vec<CpuInfo> = (0..16).map(|i| cpu(i, 0, 96)).collect();
        cpus.extend((16..32).map(|i| cpu(i, 0, 32)));
        let s = split(&cpus).unwrap();
        assert_eq!(s.kind, SplitKind::VCache);
        assert_eq!(s.game, (0..16).collect::<Vec<u32>>());
        assert_eq!(s.background, (16..32).collect::<Vec<u32>>());
    }

    #[test]
    fn homogeneous_cpus_and_small_machines_are_left_alone() {
        let ryzen_7800x3d: Vec<CpuInfo> = (0..16).map(|i| cpu(i, 0, 96)).collect();
        assert_eq!(split(&ryzen_7800x3d), None, "une seule puce : rien à répartir");
        let vm: Vec<CpuInfo> = (0..6).map(|i| cpu(i, 0, 32)).collect();
        assert_eq!(split(&vm), None);
        let small_hybrid: Vec<CpuInfo> = (0..6).map(|i| cpu(i, (i % 2) as u8, 12)).collect();
        assert_eq!(split(&small_hybrid), None, "trop peu de cœurs pour en retirer");
    }

    #[test]
    fn unknown_cache_sizes_never_trigger_a_split() {
        let cpus: Vec<CpuInfo> = (0..16).map(|i| cpu(i, 0, if i < 8 { 0 } else { 32 })).collect();
        assert_eq!(split(&cpus), None);
    }
}
