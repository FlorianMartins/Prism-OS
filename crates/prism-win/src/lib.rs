//! Exécution des plans de Prism sur Windows.
//!
//! Règles (docs/anticheat-rules.md) appliquées ici :
//! - API user-mode documentées uniquement, aucun pilote ;
//! - le jeu et les processus protégés ne sont jamais ouverts au-delà de la lecture
//!   limitée (nom, chemin, date de création, mémoire), comme le fait le Gestionnaire
//!   des tâches ;
//! - jamais de `SeDebugPrivilege` : Prism ne peut ouvrir que ce que l'utilisateur
//!   peut déjà ouvrir.

#[cfg(windows)]
mod sysconfig;
#[cfg(windows)]
mod win;

#[cfg(windows)]
pub use sysconfig::WindowsSystemConfig;

#[cfg(windows)]
pub use win::{active_power_plan, memory_lists, proc_id, process_state, MemoryLists, WindowsPlatform};

/// Format canonique d'un GUID (`8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c`).
pub fn format_guid(data1: u32, data2: u16, data3: u16, data4: [u8; 8]) -> String {
    format!(
        "{data1:08x}-{data2:04x}-{data3:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        data4[0], data4[1], data4[2], data4[3], data4[4], data4[5], data4[6], data4[7]
    )
}

/// Inverse de [`format_guid`] ; refuse tout ce qui n'est pas exactement un GUID.
pub fn parse_guid(s: &str) -> Option<(u32, u16, u16, [u8; 8])> {
    let parts: Vec<&str> = s.split('-').collect();
    let lens = [8, 4, 4, 4, 12];
    if parts.len() != 5 || parts.iter().zip(lens).any(|(p, l)| p.len() != l) {
        return None;
    }
    if !s.chars().all(|c| c == '-' || c.is_ascii_hexdigit()) {
        return None;
    }
    let data1 = u32::from_str_radix(parts[0], 16).ok()?;
    let data2 = u16::from_str_radix(parts[1], 16).ok()?;
    let data3 = u16::from_str_radix(parts[2], 16).ok()?;
    let tail = format!("{}{}", parts[3], parts[4]);
    let mut data4 = [0u8; 8];
    for (i, b) in data4.iter_mut().enumerate() {
        *b = u8::from_str_radix(&tail[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some((data1, data2, data3, data4))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guid_round_trip() {
        let s = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";
        let (a, b, c, d) = parse_guid(s).unwrap();
        assert_eq!(a, 0x8c5e7fda);
        assert_eq!(format_guid(a, b, c, d), s);
    }

    #[test]
    fn guid_rejects_garbage() {
        for bad in [
            "",
            "8c5e7fda",
            "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635",
            "zc5e7fda-e8bf-4a96-9a85-a6e23a8c635c",
        ] {
            assert!(parse_guid(bad).is_none(), "{bad}");
        }
    }
}
