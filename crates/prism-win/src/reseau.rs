//! Réseau sous Windows : ping (ICMP, sans droits administrateur), cartes actives et
//! leur passerelle, serveur du jeu (table TCP, lecture seule — comme `netstat`),
//! registre pour les réglages de `prism_core::reseau`, recherches Wi-Fi en arrière-plan.

use std::net::{Ipv4Addr, ToSocketAddrs};
use std::ptr::{null, null_mut};

use prism_core::allege::RegData;
use prism_core::reseau::Registre;
use windows_sys::core::GUID;
use windows_sys::Win32::NetworkManagement::IpHelper::{
    GetAdaptersAddresses, GetExtendedTcpTable, IcmpCloseHandle, IcmpCreateFile, IcmpSendEcho,
    GAA_FLAG_INCLUDE_GATEWAYS, ICMP_ECHO_REPLY, IP_ADAPTER_ADDRESSES_LH, MIB_TCPROW_OWNER_PID,
    TCP_TABLE_OWNER_PID_CONNECTIONS,
};
use windows_sys::Win32::NetworkManagement::WiFi::{
    wlan_interface_state_connected, wlan_intf_opcode_background_scan_enabled, WlanCloseHandle, WlanEnumInterfaces,
    WlanFreeMemory, WlanOpenHandle, WlanSetInterface, WLAN_INTERFACE_INFO_LIST,
};

const AF_INET: u32 = 2;
const IF_TYPE_IEEE80211: u32 = 71;
const IF_TYPE_SOFTWARE_LOOPBACK: u32 = 24;
const IF_TYPE_TUNNEL: u32 = 131;
const IF_OPER_UP: i32 = 1;
const MIB_TCP_STATE_ESTAB: u32 = 5;

/// Résout un nom ou une adresse en IPv4.
pub fn resoudre(hote: &str) -> Option<Ipv4Addr> {
    if let Ok(ip) = hote.parse() {
        return Some(ip);
    }
    (hote, 0).to_socket_addrs().ok()?.find_map(|a| match a.ip() {
        std::net::IpAddr::V4(v4) => Some(v4),
        _ => None,
    })
}

/// Pings successifs (un toutes les `pause_ms`) : temps en ms, `None` si pas de réponse.
pub fn pings(ip: Ipv4Addr, n: usize, pause_ms: u64) -> Vec<Option<u32>> {
    // SAFETY: handle ICMP ouvert puis fermé ici ; tampons locaux de taille annoncée.
    unsafe {
        let h = IcmpCreateFile();
        if h.is_null() || h as isize == -1 {
            return vec![None; n];
        }
        let data = [0x50u8; 32];
        let mut reply = vec![0u8; std::mem::size_of::<ICMP_ECHO_REPLY>() + data.len() + 16];
        let dest = u32::from_ne_bytes(ip.octets());
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            if i > 0 {
                std::thread::sleep(std::time::Duration::from_millis(pause_ms));
            }
            let got = IcmpSendEcho(
                h,
                dest,
                data.as_ptr() as *const _,
                data.len() as u16,
                null(),
                reply.as_mut_ptr() as *mut _,
                reply.len() as u32,
                1000,
            );
            let r = &*(reply.as_ptr() as *const ICMP_ECHO_REPLY);
            out.push((got > 0 && r.Status == 0).then_some(r.RoundTripTime));
        }
        IcmpCloseHandle(h);
        out
    }
}

/// Carte réseau active.
#[derive(Clone, Debug)]
pub struct Carte {
    /// `{…}` : nom de sa clé sous `Tcpip\Parameters\Interfaces`.
    pub guid: String,
    pub nom: String,
    pub wifi: bool,
    pub passerelle: Option<Ipv4Addr>,
}

/// Cartes réseau en service (ni boucle locale ni tunnel), celles qui ont une passerelle
/// d'abord (celle qui mène à Internet).
pub fn cartes() -> Vec<Carte> {
    let mut out = Vec::new();
    // SAFETY: premier appel pour la taille, second dans un tampon aligné ; la liste
    // chaînée pointe dans ce tampon, lue avant sa libération.
    unsafe {
        let mut size = 0u32;
        GetAdaptersAddresses(AF_INET, GAA_FLAG_INCLUDE_GATEWAYS, null(), null_mut(), &mut size);
        if size == 0 {
            return out;
        }
        let mut buf: Vec<u64> = vec![0; (size as usize).div_ceil(8) + 1];
        let first = buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH;
        if GetAdaptersAddresses(AF_INET, GAA_FLAG_INCLUDE_GATEWAYS, null(), first, &mut size) != 0 {
            return out;
        }
        let mut a = first;
        while !a.is_null() {
            let ad = &*a;
            a = ad.Next;
            if ad.OperStatus != IF_OPER_UP || ad.IfType == IF_TYPE_SOFTWARE_LOOPBACK || ad.IfType == IF_TYPE_TUNNEL {
                continue;
            }
            let guid = std::ffi::CStr::from_ptr(ad.AdapterName as *const _)
                .to_string_lossy()
                .into_owned();
            let nom = if ad.FriendlyName.is_null() {
                String::new()
            } else {
                let len = (0..).take_while(|i| *ad.FriendlyName.add(*i) != 0).count();
                String::from_utf16_lossy(std::slice::from_raw_parts(ad.FriendlyName, len))
            };
            let mut passerelle = None;
            let mut g = ad.FirstGatewayAddress;
            while !g.is_null() && passerelle.is_none() {
                let sa = (*g).Address.lpSockaddr as *const u8;
                if !sa.is_null() && u16::from_ne_bytes([*sa, *sa.add(1)]) == AF_INET as u16 {
                    passerelle = Some(Ipv4Addr::new(*sa.add(4), *sa.add(5), *sa.add(6), *sa.add(7)));
                }
                g = (*g).Next;
            }
            out.push(Carte {
                guid,
                nom,
                wifi: ad.IfType == IF_TYPE_IEEE80211,
                passerelle,
            });
        }
    }
    out.sort_by_key(|c| c.passerelle.is_none());
    out
}

fn publique(ip: Ipv4Addr) -> bool {
    !(ip.is_private() || ip.is_loopback() || ip.is_link_local() || ip.is_unspecified() || ip.is_multicast())
}

/// Serveur auquel le processus `pid` (le jeu) est connecté en TCP, s'il y en a un
/// (adresse publique). Les jeux en UDP n'apparaissent pas ici.
pub fn serveur_du_jeu(pid: u32) -> Option<Ipv4Addr> {
    // SAFETY: premier appel pour la taille, second dans un tampon aligné lu aussitôt.
    unsafe {
        let mut size = 0u32;
        GetExtendedTcpTable(null_mut(), &mut size, 0, AF_INET, TCP_TABLE_OWNER_PID_CONNECTIONS, 0);
        if size == 0 {
            return None;
        }
        let mut buf: Vec<u32> = vec![0; (size as usize).div_ceil(4) + 1];
        if GetExtendedTcpTable(
            buf.as_mut_ptr() as *mut _,
            &mut size,
            0,
            AF_INET,
            TCP_TABLE_OWNER_PID_CONNECTIONS,
            0,
        ) != 0
        {
            return None;
        }
        let n = buf[0] as usize;
        let rows = std::slice::from_raw_parts(buf.as_ptr().add(1) as *const MIB_TCPROW_OWNER_PID, n);
        rows.iter()
            .filter(|r| r.dwOwningPid == pid && r.dwState == MIB_TCP_STATE_ESTAB)
            .map(|r| Ipv4Addr::from(r.dwRemoteAddr.to_ne_bytes()))
            .find(|ip| publique(*ip))
    }
}

/// Registre de la machine (HKLM) pour les réglages réseau.
pub struct RegistreWin;

impl Registre for RegistreWin {
    fn lire(&mut self, cle: &str, valeur: &str) -> Option<u32> {
        match crate::sysconfig::reg_get(&format!("HKLM\\{cle}"), valeur) {
            Ok(Some(RegData::Dword(d))) => Some(d),
            _ => None,
        }
    }
    fn ecrire(&mut self, cle: &str, valeur: &str, donnee: u32) -> Result<(), String> {
        crate::sysconfig::reg_set(&format!("HKLM\\{cle}"), valeur, &RegData::Dword(donnee))
    }
    fn effacer(&mut self, cle: &str, valeur: &str) -> Result<(), String> {
        crate::sysconfig::reg_delete(&format!("HKLM\\{cle}"), valeur)
    }
}

/// Recherches Wi-Fi en arrière-plan suspendues sur les cartes connectées, tant que cette
/// valeur vit : Windows cherche sinon d'autres réseaux toutes les minutes environ, ce
/// qui fait sauter le ping. Rétablies par [`WifiPause::reprendre`] ou à la destruction.
pub struct WifiPause {
    handle: isize,
    cartes: Vec<GUID>,
}

fn wlan_open() -> Option<isize> {
    let mut version = 0u32;
    let mut h = null_mut();
    // SAFETY: sorties locales.
    (unsafe { WlanOpenHandle(2, null(), &mut version, &mut h) } == 0).then_some(h as isize)
}

/// Cartes Wi-Fi (toutes, ou seulement connectées).
fn wlan_cartes(h: isize, connectees: bool) -> Vec<GUID> {
    let mut list: *mut WLAN_INTERFACE_INFO_LIST = null_mut();
    // SAFETY: liste allouée par Windows, lue puis libérée.
    unsafe {
        if WlanEnumInterfaces(h as _, null(), &mut list) != 0 || list.is_null() {
            return Vec::new();
        }
        let n = (*list).dwNumberOfItems as usize;
        let items = std::slice::from_raw_parts((*list).InterfaceInfo.as_ptr(), n);
        let out = items
            .iter()
            .filter(|i| !connectees || i.isState == wlan_interface_state_connected)
            .map(|i| i.InterfaceGuid)
            .collect();
        WlanFreeMemory(list as *const _);
        out
    }
}

fn recherche(h: isize, carte: &GUID, active: bool) -> bool {
    let v: i32 = active as i32;
    // SAFETY: donnée locale de 4 octets annoncés.
    unsafe {
        WlanSetInterface(
            h as _,
            carte,
            wlan_intf_opcode_background_scan_enabled,
            4,
            &v as *const i32 as *const _,
            null(),
        ) == 0
    }
}

impl WifiPause {
    /// `None` : pas de Wi-Fi connecté (câble), ou Windows refuse.
    pub fn suspendre() -> Option<WifiPause> {
        let h = wlan_open()?;
        let cartes: Vec<GUID> = wlan_cartes(h, true)
            .into_iter()
            .filter(|g| recherche(h, g, false))
            .collect();
        if cartes.is_empty() {
            // SAFETY: notre handle.
            unsafe { WlanCloseHandle(h as _, null()) };
            return None;
        }
        Some(WifiPause { handle: h, cartes })
    }

    pub fn cartes(&self) -> usize {
        self.cartes.len()
    }

    pub fn reprendre(self) {}
}

impl Drop for WifiPause {
    fn drop(&mut self) {
        for g in &self.cartes {
            recherche(self.handle, g, true);
        }
        // SAFETY: notre handle, fermé une fois.
        unsafe { WlanCloseHandle(self.handle as _, null()) };
    }
}

/// Filet de sécurité au démarrage du moteur (arrêt brutal pendant une partie) :
/// recherches rétablies sur toutes les cartes Wi-Fi.
pub fn wifi_tout_reprendre() {
    if let Some(h) = wlan_open() {
        for g in wlan_cartes(h, false) {
            recherche(h, &g, true);
        }
        // SAFETY: notre handle.
        unsafe { WlanCloseHandle(h as _, null()) };
    }
}

/// Une cible mesurée.
#[derive(Clone, Debug)]
pub struct Mesure {
    pub nom: String,
    pub ip: Option<Ipv4Addr>,
    pub stats: prism_core::reseau::Stats,
}

/// Test de la connexion : la box, Internet (Cloudflare 1.1.1.1), et les cibles en plus
/// (serveur choisi, serveur du jeu). Pings en parallèle, 2 par seconde.
#[derive(Clone, Debug)]
pub struct Test {
    pub carte: Option<Carte>,
    pub box_: Mesure,
    pub internet: Mesure,
    pub autres: Vec<Mesure>,
}

impl Test {
    pub fn diagnostic(&self) -> String {
        prism_core::reseau::diagnostic(&self.box_.stats, &self.internet.stats)
    }
}

pub fn tester(cibles: &[(String, Option<Ipv4Addr>)], n: usize) -> Test {
    let carte = cartes().into_iter().next();
    let mut todo: Vec<(String, Option<Ipv4Addr>)> = vec![
        ("Box (passerelle)".into(), carte.as_ref().and_then(|c| c.passerelle)),
        ("Internet (1.1.1.1)".into(), Some(Ipv4Addr::new(1, 1, 1, 1))),
    ];
    todo.extend(cibles.iter().cloned());
    let handles: Vec<_> = todo
        .into_iter()
        .map(|(nom, ip)| {
            std::thread::spawn(move || {
                let s = ip.map(|ip| pings(ip, n, 500)).unwrap_or_default();
                Mesure {
                    nom,
                    ip,
                    stats: prism_core::reseau::Stats::de(&s),
                }
            })
        })
        .collect();
    let mut all: Vec<Mesure> = handles.into_iter().filter_map(|h| h.join().ok()).collect();
    let autres = all.split_off(2.min(all.len()));
    let mut it = all.into_iter();
    let vide = |nom: &str| Mesure {
        nom: nom.into(),
        ip: None,
        stats: Default::default(),
    };
    Test {
        carte,
        box_: it.next().unwrap_or_else(|| vide("Box")),
        internet: it.next().unwrap_or_else(|| vide("Internet")),
        autres,
    }
}

/// Active ou retire un réglage réseau permanent (administrateur) ; journal dans
/// `%ProgramData%\Prism`.
pub fn optim(o: prism_core::reseau::Optim, on: bool) -> Result<String, String> {
    use prism_core::reseau::{appliquer, retirer, Journal};
    let dir = prism_core::paths::data_dir();
    let mut j = Journal::charger(&dir);
    let mut reg = RegistreWin;
    let r = if on {
        let ifs: Vec<String> = cartes().into_iter().map(|c| c.guid).collect();
        appliquer(&mut reg, &mut j, o, &ifs).map(|n| format!("{} : activé ({n} valeur(s))", o.label()))
    } else {
        retirer(&mut reg, &mut j, o).map(|n| format!("{} : retiré, {n} valeur(s) d'origine remise(s)", o.label()))
    };
    j.enregistrer(&dir)?;
    r
}

/// Remet les valeurs d'origine de tous les réglages réseau (« Tout restaurer »,
/// désinstallation). Rend le nombre de valeurs remises.
pub fn tout_retirer() -> Result<usize, String> {
    use prism_core::reseau::{retirer, Journal, OPTIMS};
    let dir = prism_core::paths::data_dir();
    let mut j = Journal::charger(&dir);
    let mut reg = RegistreWin;
    let mut n = 0;
    for o in OPTIMS {
        n += retirer(&mut reg, &mut j, o)?;
    }
    j.enregistrer(&dir)?;
    // Recherches Wi-Fi suspendues par un moteur arrêté brutalement en pleine partie.
    let temoin = dir.join("wifi-recherche-suspendue");
    if temoin.exists() {
        wifi_tout_reprendre();
        let _ = std::fs::remove_file(temoin);
    }
    Ok(n)
}
