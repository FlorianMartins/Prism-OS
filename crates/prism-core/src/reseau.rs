//! Réseau : mesure de la connexion (ping, gigue, pertes) avec un diagnostic qui dit où
//! se trouve le problème, et réglages réversibles qui réduisent la latence.
//!
//! Ce que Prism ne fait pas, et pourquoi : les « accélérateurs » (LagoFast, ExitLag…)
//! font passer le trafic du jeu par leurs serveurs relais, avec un pilote réseau ou un
//! VPN. Prism n'a ni serveurs ni pilote (règle anti-cheat : aucun pilote, jamais rien
//! dans le jeu). La QoS de Windows par stratégie (priorité/bridage par appli) a été
//! mesurée sans effet sur un PC hors domaine (VM, 2026-10-06) : écartée.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

// --- Mesure -------------------------------------------------------------------------

/// Résultat d'une série de pings (`None` : pas de réponse).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Stats {
    pub envoyes: u32,
    pub perdus: u32,
    pub moyenne_ms: f32,
    pub min_ms: u32,
    pub max_ms: u32,
    /// Gigue : écart moyen entre deux réponses successives (ce qui fait « sauter »).
    pub gigue_ms: f32,
}

impl Stats {
    pub fn de(echantillons: &[Option<u32>]) -> Stats {
        let ok: Vec<u32> = echantillons.iter().flatten().copied().collect();
        let mut s = Stats {
            envoyes: echantillons.len() as u32,
            perdus: (echantillons.len() - ok.len()) as u32,
            ..Default::default()
        };
        if ok.is_empty() {
            return s;
        }
        s.moyenne_ms = ok.iter().sum::<u32>() as f32 / ok.len() as f32;
        s.min_ms = *ok.iter().min().unwrap_or(&0);
        s.max_ms = *ok.iter().max().unwrap_or(&0);
        if ok.len() > 1 {
            s.gigue_ms = ok.windows(2).map(|w| w[0].abs_diff(w[1])).sum::<u32>() as f32 / (ok.len() - 1) as f32;
        }
        s
    }

    pub fn pertes_pourcent(&self) -> f32 {
        if self.envoyes == 0 {
            return 0.0;
        }
        self.perdus as f32 * 100.0 / self.envoyes as f32
    }

    pub fn joignable(&self) -> bool {
        self.envoyes > self.perdus
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Qualite {
    Excellente,
    Bonne,
    Moyenne,
    Mauvaise,
}

impl Qualite {
    pub fn label(self) -> &'static str {
        match self {
            Qualite::Excellente => "excellente",
            Qualite::Bonne => "bonne",
            Qualite::Moyenne => "moyenne",
            Qualite::Mauvaise => "mauvaise",
        }
    }
}

/// Note d'une liaison pour le jeu en ligne : la gigue et les pertes comptent autant que
/// le ping (un ping stable à 40 ms se joue mieux qu'un ping qui saute de 15 à 90).
pub fn qualite(s: &Stats) -> Qualite {
    if !s.joignable() {
        return Qualite::Mauvaise;
    }
    let (p, g, l) = (s.moyenne_ms, s.gigue_ms, s.pertes_pourcent());
    if l >= 5.0 || p >= 150.0 || g >= 30.0 {
        Qualite::Mauvaise
    } else if l >= 1.0 || p >= 80.0 || g >= 12.0 {
        Qualite::Moyenne
    } else if p >= 35.0 || g >= 5.0 {
        Qualite::Bonne
    } else {
        Qualite::Excellente
    }
}

/// Où se trouve le problème : la box (passerelle) mesure le réseau local (câble ou
/// Wi-Fi), Internet mesure l'accès du fournisseur.
pub fn diagnostic(box_: &Stats, internet: &Stats) -> String {
    let local = qualite(box_);
    let net = qualite(internet);
    if !box_.joignable() && !internet.joignable() {
        return "Aucune réponse : pas de connexion, ou un pare-feu bloque le ping.".into();
    }
    if local >= Qualite::Moyenne {
        return format!(
            "Le problème est chez vous, entre le PC et la box ({} ms, gigue {:.0} ms, {:.0} % de pertes) : \
             en Wi-Fi, rapprochez-vous de la box, passez en 5 GHz ou, mieux, branchez un câble.",
            box_.moyenne_ms.round(),
            box_.gigue_ms,
            box_.pertes_pourcent()
        );
    }
    if net >= Qualite::Moyenne {
        return format!(
            "Votre réseau local est sain ; le problème est au-delà de la box ({} ms, gigue {:.0} ms, {:.0} % \
             de pertes) : un autre appareil qui télécharge, ou votre fournisseur.",
            internet.moyenne_ms.round(),
            internet.gigue_ms,
            internet.pertes_pourcent()
        );
    }
    format!(
        "Connexion {} pour le jeu : {} ms vers Internet, gigue {:.0} ms, aucune perte notable.",
        net.label(),
        internet.moyenne_ms.round(),
        internet.gigue_ms
    )
}

// --- Réglages de l'utilisateur ------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Reglages {
    /// En Wi-Fi, pendant une partie : pas de recherche de réseaux en arrière-plan.
    pub wifi_sans_recherche_en_jeu: bool,
    /// Serveur à mesurer en plus (nom ou adresse), vide : aucun.
    pub cible: String,
}

impl Reglages {
    pub const FICHIER: &'static str = "reseau.json";

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

// --- Réglages permanents (registre), réversibles ------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Optim {
    /// Windows limite le trafic réseau des applis non multimédia quand un son ou une
    /// vidéo joue (`NetworkThrottlingIndex`, 10 paquets/ms par défaut) : levée.
    SansLimitation,
    /// Accusés de réception TCP immédiats et envoi sans regroupement (Nagle) sur les
    /// cartes réseau : utile aux jeux qui passent par TCP (la plupart utilisent UDP).
    TcpImmediat,
}

pub const OPTIMS: [Optim; 2] = [Optim::SansLimitation, Optim::TcpImmediat];

impl Optim {
    pub fn id(self) -> &'static str {
        match self {
            Optim::SansLimitation => "limitation",
            Optim::TcpImmediat => "tcp",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Optim::SansLimitation => "Lever la limitation réseau multimédia de Windows",
            Optim::TcpImmediat => "Accusés TCP immédiats (Nagle désactivé)",
        }
    }

    pub fn pourquoi(self) -> &'static str {
        match self {
            Optim::SansLimitation => {
                "Quand un son ou une vidéo joue (Discord, YouTube), Windows bride le trafic réseau des autres \
                 applis à 10 paquets par milliseconde. Levé : le jeu n'est plus bridé."
            }
            Optim::TcpImmediat => {
                "Windows regroupe les petits paquets TCP et retarde ses accusés de réception (jusqu'à 200 ms). \
                 Utile aux jeux et lanceurs qui passent par TCP ; sans effet sur les jeux en UDP."
            }
        }
    }

    /// Valeurs écrites : (clé sous HKLM, valeur, donnée DWORD). `interfaces` : GUID des
    /// cartes réseau (`{…}`), pour les réglages TCP.
    pub fn ecritures(self, interfaces: &[String]) -> Vec<(String, String, u32)> {
        match self {
            Optim::SansLimitation => vec![(
                r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile".into(),
                "NetworkThrottlingIndex".into(),
                0xFFFF_FFFF,
            )],
            Optim::TcpImmediat => interfaces
                .iter()
                .flat_map(|g| {
                    let k = format!(r"SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\{g}");
                    [(k.clone(), "TcpAckFrequency".into(), 1), (k, "TCPNoDelay".into(), 1)]
                })
                .collect(),
        }
    }
}

/// Seules ces clés (sous HKLM, en minuscules) peuvent être écrites par le réseau.
pub const CLES_AUTORISEES: [&str; 2] = [
    r"software\microsoft\windows nt\currentversion\multimedia\systemprofile",
    r"system\currentcontrolset\services\tcpip\parameters\interfaces\",
];

pub fn cle_autorisee(cle: &str) -> bool {
    let k = cle.to_ascii_lowercase();
    CLES_AUTORISEES
        .iter()
        .any(|a| k == *a || (a.ends_with('\\') && k.starts_with(a)))
}

/// Accès au registre (HKLM, DWORD), simulable pour les tests.
pub trait Registre {
    fn lire(&mut self, cle: &str, valeur: &str) -> Option<u32>;
    fn ecrire(&mut self, cle: &str, valeur: &str, donnee: u32) -> Result<(), String>;
    fn effacer(&mut self, cle: &str, valeur: &str) -> Result<(), String>;
}

/// Valeurs d'origine (`None` : absente), pour tout remettre exactement.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Journal {
    /// Par réglage (`Optim::id`) : « clé\valeur » → donnée d'origine.
    pub origines: BTreeMap<String, BTreeMap<String, Option<u32>>>,
}

impl Journal {
    pub const FICHIER: &'static str = "reseau-journal.json";

    pub fn charger(dir: &Path) -> Journal {
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

    pub fn actif(&self, o: Optim) -> bool {
        self.origines.contains_key(o.id())
    }
}

fn split(k: &str) -> (&str, &str) {
    k.rsplit_once('\\').unwrap_or((k, ""))
}

/// Applique un réglage ; l'origine de chaque valeur est notée une seule fois.
pub fn appliquer(reg: &mut dyn Registre, j: &mut Journal, o: Optim, interfaces: &[String]) -> Result<usize, String> {
    let ecr = o.ecritures(interfaces);
    if ecr.is_empty() {
        return Err("aucune carte réseau active".into());
    }
    let orig = j.origines.entry(o.id().to_string()).or_default();
    let mut n = 0;
    for (cle, valeur, donnee) in ecr {
        if !cle_autorisee(&cle) {
            return Err(format!("{cle} : clé hors de la liste autorisée"));
        }
        let id = format!("{cle}\\{valeur}");
        orig.entry(id).or_insert_with(|| reg.lire(&cle, &valeur));
        reg.ecrire(&cle, &valeur, donnee)?;
        n += 1;
    }
    Ok(n)
}

/// Remet les valeurs d'origine d'un réglage (effacées si elles n'existaient pas).
pub fn retirer(reg: &mut dyn Registre, j: &mut Journal, o: Optim) -> Result<usize, String> {
    let Some(orig) = j.origines.remove(o.id()) else {
        return Ok(0);
    };
    let mut n = 0;
    for (id, avant) in orig {
        let (cle, valeur) = split(&id);
        match avant {
            Some(d) => reg.ecrire(cle, valeur, d)?,
            None => reg.effacer(cle, valeur)?,
        }
        n += 1;
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Faux(HashMap<String, u32>);

    impl Registre for Faux {
        fn lire(&mut self, cle: &str, valeur: &str) -> Option<u32> {
            self.0.get(&format!("{cle}\\{valeur}")).copied()
        }
        fn ecrire(&mut self, cle: &str, valeur: &str, donnee: u32) -> Result<(), String> {
            self.0.insert(format!("{cle}\\{valeur}"), donnee);
            Ok(())
        }
        fn effacer(&mut self, cle: &str, valeur: &str) -> Result<(), String> {
            self.0.remove(&format!("{cle}\\{valeur}"));
            Ok(())
        }
    }

    #[test]
    fn stats_count_losses_and_jitter() {
        let s = Stats::de(&[Some(20), Some(30), None, Some(20), Some(40)]);
        assert_eq!((s.envoyes, s.perdus, s.min_ms, s.max_ms), (5, 1, 20, 40));
        assert_eq!(s.moyenne_ms, 27.5);
        assert!((s.gigue_ms - 13.333).abs() < 0.01);
        assert_eq!(s.pertes_pourcent(), 20.0);
        assert!(!Stats::de(&[None, None]).joignable());
    }

    #[test]
    fn quality_weighs_jitter_and_losses_like_ping() {
        let stable = Stats::de(&[Some(40); 10]);
        let saute = Stats::de(&[Some(15), Some(60), Some(15), Some(60)]);
        assert_eq!(qualite(&stable), Qualite::Bonne);
        assert_eq!(qualite(&saute), Qualite::Mauvaise);
        assert_eq!(qualite(&Stats::de(&[Some(8); 10])), Qualite::Excellente);
    }

    #[test]
    fn diagnosis_points_at_the_local_network_or_beyond_the_box() {
        let bon = Stats::de(&[Some(2); 10]);
        let wifi = Stats::de(&[Some(3), Some(45), Some(4), None, Some(50), Some(3)]);
        let net = Stats::de(&[Some(25); 10]);
        let lent = Stats::de(&[Some(25), Some(90), Some(30), Some(95)]);
        assert!(diagnostic(&wifi, &net).contains("chez vous"));
        assert!(diagnostic(&bon, &lent).contains("au-delà de la box"));
        assert!(diagnostic(&bon, &net).starts_with("Connexion excellente"));
    }

    #[test]
    fn optimisations_apply_and_restore_exactly() {
        let mut reg = Faux::default();
        let lim = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\NetworkThrottlingIndex";
        reg.0.insert(lim.into(), 10);
        let ifs = vec!["{AAAA}".to_string()];
        let mut j = Journal::default();
        appliquer(&mut reg, &mut j, Optim::SansLimitation, &ifs).unwrap();
        assert_eq!(appliquer(&mut reg, &mut j, Optim::TcpImmediat, &ifs).unwrap(), 2);
        assert_eq!(reg.0[lim], 0xFFFF_FFFF);
        let ack = r"SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\{AAAA}\TcpAckFrequency";
        assert_eq!(reg.0[ack], 1);
        // Réappliqué : l'origine notée reste la toute première.
        appliquer(&mut reg, &mut j, Optim::SansLimitation, &ifs).unwrap();
        assert!(j.actif(Optim::TcpImmediat));
        retirer(&mut reg, &mut j, Optim::SansLimitation).unwrap();
        retirer(&mut reg, &mut j, Optim::TcpImmediat).unwrap();
        assert_eq!(reg.0[lim], 10, "valeur d'origine remise");
        assert!(!reg.0.contains_key(ack), "valeur absente avant : effacée");
        assert!(j.origines.is_empty());
    }

    #[test]
    fn only_network_keys_can_be_written() {
        assert!(cle_autorisee(
            r"SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\{AAAA}"
        ));
        assert!(!cle_autorisee(r"SYSTEM\CurrentControlSet\Services\Tcpip\Parameters"));
        assert!(!cle_autorisee(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run"));
    }
}
