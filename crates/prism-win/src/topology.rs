//! Topologie du processeur : CPU sets (classe d'efficacité) et taille du cache L3 de
//! chaque cœur logique.

use std::collections::HashMap;
use std::ptr::null_mut;

use prism_core::model::CpuInfo;
use windows_sys::Win32::System::SystemInformation::{
    CpuSetInformation, GetLogicalProcessorInformationEx, GetSystemCpuSetInformation, RelationCache,
    SYSTEM_CPU_SET_INFORMATION, SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
};

/// (groupe, index du processeur logique) -> taille du cache L3 en octets.
fn l3_sizes() -> HashMap<(u16, u8), u64> {
    let mut out = HashMap::new();
    // SAFETY: premier appel pour la taille ; tampon aligné sur 8 ; on parcourt les
    // entrées de taille variable en respectant leur champ `Size`.
    unsafe {
        let mut len = 0u32;
        GetLogicalProcessorInformationEx(RelationCache, null_mut(), &mut len);
        if len == 0 {
            return out;
        }
        let mut buf = vec![0u64; (len as usize).div_ceil(8)];
        let base = buf.as_mut_ptr() as *mut u8;
        if GetLogicalProcessorInformationEx(
            RelationCache,
            base as *mut SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
            &mut len,
        ) == 0
        {
            return out;
        }
        let mut off = 0usize;
        while off < len as usize {
            let e = &*(base.add(off) as *const SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX);
            if e.Size == 0 {
                break;
            }
            if e.Relationship == RelationCache {
                let c = &e.Anonymous.Cache;
                if c.Level == 3 {
                    let gm = c.Anonymous.GroupMask;
                    for bit in 0..(usize::BITS as u8) {
                        if gm.Mask & (1usize << bit) != 0 {
                            out.insert((gm.Group, bit), c.CacheSize as u64);
                        }
                    }
                }
            }
            off += e.Size as usize;
        }
    }
    out
}

/// Les CPU sets de la machine, avec leur classe d'efficacité et leur cache L3.
pub fn cpus() -> Vec<CpuInfo> {
    let l3 = l3_sizes();
    let mut out = Vec::new();
    // SAFETY: mêmes précautions que ci-dessus ; pseudo-handle nul = toute la machine.
    unsafe {
        let mut len = 0u32;
        GetSystemCpuSetInformation(null_mut(), 0, &mut len, null_mut(), 0);
        if len == 0 {
            return out;
        }
        let mut buf = vec![0u64; (len as usize).div_ceil(8)];
        let base = buf.as_mut_ptr() as *mut u8;
        if GetSystemCpuSetInformation(base as *mut SYSTEM_CPU_SET_INFORMATION, len, &mut len, null_mut(), 0) == 0 {
            return out;
        }
        let mut off = 0usize;
        while off < len as usize {
            let e = &*(base.add(off) as *const SYSTEM_CPU_SET_INFORMATION);
            if e.Size == 0 {
                break;
            }
            if e.Type == CpuSetInformation {
                let c = &e.Anonymous.CpuSet;
                out.push(CpuInfo {
                    id: c.Id,
                    efficiency_class: c.EfficiencyClass,
                    llc_bytes: l3.get(&(c.Group, c.LogicalProcessorIndex)).copied().unwrap_or(0),
                });
            }
            off += e.Size as usize;
        }
    }
    out
}
