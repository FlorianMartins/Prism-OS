//! `prism rapport` : où part la mémoire de cette machine, en texte simple, à lire ou à
//! envoyer. Aucune donnée personnelle : ni nom d'utilisateur, ni nom du PC, ni chemin
//! de fichier — des noms de programmes et de services, et des chiffres.
//!
//! La collecte est faite par la plateforme (`prism-win`) ; ici, la mise en forme et
//! les conseils, testés.

const MO: u64 = 1 << 20;

/// Chiffres de la mémoire physique et virtuelle, en octets (0 : inconnu).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Memoire {
    /// RAM installée (barrettes) ; la différence avec `totale` est réservée par le
    /// matériel (carte graphique intégrée, BIOS).
    pub installee: u64,
    /// RAM utilisable par Windows.
    pub totale: u64,
    pub disponible: u64,
    pub engagee: u64,
    pub limite_engagee: u64,
    pub pool_pagine: u64,
    pub pool_non_pagine: u64,
    /// Cache en attente (libéré à la demande), liste modifiée, pages libres.
    pub cache: u64,
    pub modifiee: u64,
    pub libre: u64,
    /// Mémoire compressée par Windows (processus « Memory Compression »).
    pub compressee: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Processus {
    pub nom: String,
    pub session: u32,
    /// Mémoire privée engagée (la sienne, en RAM ou dans le fichier d'échange).
    pub prive: u64,
    /// Mémoire de travail (ce qui est en RAM maintenant).
    pub ws: u64,
}

/// Hôte de services (`svchost.exe`) et les services qu'il fait tourner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hote {
    pub ws: u64,
    pub prive: u64,
    pub services: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Donnees {
    pub date: String,
    pub prism: String,
    pub windows: String,
    pub processeur: String,
    pub allume_depuis_secs: u64,
    pub memoire: Memoire,
    pub processus: Vec<Processus>,
    pub hotes: Vec<Hote>,
    /// Applis et mémoire de leurs WebView, fenêtre ouverte ou non.
    pub webviews: Vec<(String, u64, bool)>,
    /// Applis activées au démarrage.
    pub demarrage: Vec<String>,
    /// État de Prism (niveaux d'allègement, plans), une ligne chacun.
    pub etat_prism: Vec<String>,
}

/// Une appli : ses processus additionnés.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Appli {
    pub nom: String,
    pub nombre: usize,
    pub prive: u64,
    pub ws: u64,
}

pub fn par_appli(procs: &[Processus]) -> Vec<Appli> {
    let mut map: std::collections::BTreeMap<String, Appli> = std::collections::BTreeMap::new();
    for p in procs {
        let a = map.entry(p.nom.to_lowercase()).or_insert_with(|| Appli {
            nom: p.nom.clone(),
            nombre: 0,
            prive: 0,
            ws: 0,
        });
        a.nombre += 1;
        a.prive += p.prive;
        a.ws += p.ws;
    }
    let mut out: Vec<Appli> = map.into_values().collect();
    // Classées par mémoire en RAM : une mémoire privée partie dans le fichier d'échange
    // (mesuré : un Bloc-notes à 612 Mo privés pour 5 Mo en RAM) n'occupe pas la RAM.
    out.sort_by_key(|a| std::cmp::Reverse(a.ws));
    out
}

fn go(b: u64) -> String {
    format!("{:.1} Go", b as f64 / (1u64 << 30) as f64)
}

fn mo(b: u64) -> String {
    format!("{} Mo", b / MO)
}

/// Conseils tirés des chiffres, du plus important au moins important.
pub fn conseils(d: &Donnees) -> Vec<String> {
    let m = &d.memoire;
    let mut out = Vec::new();
    let reserve = m.installee.saturating_sub(m.totale);
    if m.installee > 0 && reserve > 512 * MO {
        out.push(format!(
            "{} sont réservés par le matériel avant même le démarrage de Windows (carte graphique intégrée, BIOS) : Windows ne les voit pas. Si vous avez une carte graphique dédiée, réduire la mémoire de la carte intégrée dans le BIOS (« UMA Frame Buffer », « iGPU Memory ») les rend.",
            go(reserve)
        ));
    }
    if m.pool_non_pagine > 1024 * MO {
        out.push(format!(
            "Mémoire non paginée du noyau anormalement haute ({}) : signe classique d'un pilote qui fuit (réseau Killer/Realtek/Intel, antivirus, logiciels RGB ou de périphériques). La valeur normale est de 200 à 600 Mo. Mettre à jour les pilotes réseau, ou désinstaller le logiciel suspect, puis comparer avec un nouveau rapport.",
            mo(m.pool_non_pagine)
        ));
    }
    if m.pool_pagine > 2048 * MO {
        out.push(format!(
            "Mémoire paginée du noyau haute ({}) : souvent un pilote ou un logiciel qui surveille les fichiers (antivirus tiers, synchronisation cloud).",
            mo(m.pool_pagine)
        ));
    }
    if m.compressee > 1024 * MO {
        out.push(format!(
            "Windows compresse {} de mémoire : la RAM est pleine par moments. Les applis ci-dessous sont celles à fermer ou à retirer du démarrage.",
            go(m.compressee)
        ));
    }
    if m.limite_engagee > 0 && m.engagee * 100 / m.limite_engagee >= 85 {
        out.push(format!(
            "Mémoire engagée à {} % de la limite ({} sur {}) : le fichier d'échange travaille, la machine ralentit.",
            m.engagee * 100 / m.limite_engagee,
            go(m.engagee),
            go(m.limite_engagee)
        ));
    }
    let wv: u64 = d.webviews.iter().map(|w| w.1).sum();
    if wv > 300 * MO {
        let noms: Vec<String> = d
            .webviews
            .iter()
            .take(4)
            .map(|w| format!("{} ({})", w.0, mo(w.1)))
            .collect();
        out.push(format!(
            "{} de WebView (applis web déguisées) : {}. Celles sans fenêtre sont fermées par Prism (« WebView en arrière-plan ») ; Teams et quelques autres recréent la leur : les quitter.",
            mo(wv),
            noms.join(", ")
        ));
    }
    let lourdes: Vec<String> = par_appli(&d.processus)
        .into_iter()
        .filter(|a| !SYSTEME.contains(&a.nom.to_lowercase().as_str()) && a.ws >= 400 * MO)
        .take(5)
        .map(|a| format!("{} ({})", a.nom, mo(a.ws)))
        .collect();
    if !lourdes.is_empty() {
        out.push(format!("Applis les plus lourdes : {}.", lourdes.join(", ")));
    }
    if d.demarrage.len() > 8 {
        out.push(format!(
            "{} applis se lancent avec Windows : la page Démarrage de Prism dit lesquelles sont utiles (c'est le plus gros gain mesuré : jusqu'à 1,2 Go).",
            d.demarrage.len()
        ));
    }
    if m.cache > 2048 * MO {
        out.push(format!(
            "{} sont du cache (fichiers récemment lus) : Windows le rend instantanément à une appli qui en a besoin. Ce n'est pas de la RAM perdue ; le Gestionnaire des tâches le compte dans « Disponible ».",
            go(m.cache)
        ));
    }
    out
}

/// Processus du système : jamais proposés comme « applis lourdes ».
const SYSTEME: [&str; 9] = [
    "msedgewebview2.exe",
    "memory compression",
    "system",
    "registry",
    "svchost.exe",
    "msmpeng.exe",
    "dwm.exe",
    "explorer.exe",
    "csrss.exe",
];

pub fn texte(d: &Donnees) -> String {
    use std::fmt::Write;
    let m = &d.memoire;
    let mut t = String::new();
    let _ = writeln!(t, "RAPPORT MÉMOIRE PRISM — {}", d.date);
    let _ = writeln!(
        t,
        "(aucune donnée personnelle : ni nom d'utilisateur, ni nom du PC, ni fichier)"
    );
    let _ = writeln!(t);
    let _ = writeln!(t, "Machine");
    let _ = writeln!(t, "  {}", d.windows);
    let _ = writeln!(t, "  {}", d.processeur);
    let _ = writeln!(
        t,
        "  allumé depuis {} h {:02} min · Prism {}",
        d.allume_depuis_secs / 3600,
        d.allume_depuis_secs % 3600 / 60,
        d.prism
    );
    let _ = writeln!(t);
    let _ = writeln!(t, "Mémoire");
    if m.installee > 0 {
        let _ = writeln!(
            t,
            "  installée {:>10}   utilisable {}   réservée par le matériel {}",
            go(m.installee),
            go(m.totale),
            go(m.installee.saturating_sub(m.totale))
        );
    } else {
        let _ = writeln!(t, "  utilisable {}", go(m.totale));
    }
    let _ = writeln!(
        t,
        "  utilisée   {:>10}   disponible {}",
        go(m.totale.saturating_sub(m.disponible)),
        go(m.disponible)
    );
    let _ = writeln!(
        t,
        "  cache      {:>10}   modifiée {}   libre {}   compressée {}",
        go(m.cache),
        mo(m.modifiee),
        mo(m.libre),
        mo(m.compressee)
    );
    let _ = writeln!(
        t,
        "  noyau      paginée {}   non paginée {}",
        mo(m.pool_pagine),
        mo(m.pool_non_pagine)
    );
    let _ = writeln!(t, "  engagée    {:>10} sur {}", go(m.engagee), go(m.limite_engagee));
    let _ = writeln!(t);
    let _ = writeln!(t, "Conseils");
    let c = conseils(d);
    if c.is_empty() {
        let _ = writeln!(t, "  Rien d'anormal.");
    }
    for (i, l) in c.iter().enumerate() {
        let _ = writeln!(t, "  {}. {l}", i + 1);
    }
    let _ = writeln!(t);
    let _ = writeln!(t, "Applis (processus additionnés) — en RAM / privée");
    for a in par_appli(&d.processus).iter().take(30) {
        let _ = writeln!(
            t,
            "  {:<34} {:>3} proc  {:>7}  {:>7}",
            a.nom,
            a.nombre,
            mo(a.ws),
            mo(a.prive)
        );
    }
    let _ = writeln!(t);
    let _ = writeln!(t, "Hôtes de services (svchost) les plus lourds");
    let mut hotes = d.hotes.clone();
    hotes.sort_by_key(|h| std::cmp::Reverse(h.ws));
    for h in hotes.iter().take(15) {
        let _ = writeln!(t, "  {:>7}  {}", mo(h.ws), h.services.join(", "));
    }
    let services: usize = d.hotes.iter().map(|h| h.services.len()).sum();
    let _ = writeln!(t, "  ({} hôtes, {services} services en marche)", d.hotes.len());
    let _ = writeln!(t);
    let _ = writeln!(t, "WebView par appli");
    if d.webviews.is_empty() {
        let _ = writeln!(t, "  aucune");
    }
    for (nom, b, fenetre) in &d.webviews {
        let _ = writeln!(
            t,
            "  {:<34} {:>7}  {}",
            nom,
            mo(*b),
            if *fenetre { "fenêtre ouverte" } else { "sans fenêtre" }
        );
    }
    let _ = writeln!(t);
    let _ = writeln!(t, "Au démarrage de Windows ({})", d.demarrage.len());
    for n in &d.demarrage {
        let _ = writeln!(t, "  {n}");
    }
    let _ = writeln!(t);
    let _ = writeln!(t, "Prism");
    for l in &d.etat_prism {
        let _ = writeln!(t, "  {l}");
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(nom: &str, mb: u64) -> Processus {
        Processus {
            nom: nom.into(),
            session: 1,
            prive: mb * MO,
            ws: mb * MO,
        }
    }

    fn base() -> Donnees {
        Donnees {
            memoire: Memoire {
                installee: 16 << 30,
                totale: 16 << 30,
                disponible: 8 << 30,
                engagee: 8 << 30,
                limite_engagee: 20 << 30,
                pool_pagine: 500 * MO,
                pool_non_pagine: 300 * MO,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn a_healthy_machine_gets_no_alarm() {
        assert!(conseils(&base()).is_empty());
    }

    #[test]
    fn driver_leak_and_reserved_memory_are_explained_first() {
        let mut d = base();
        d.memoire.pool_non_pagine = 3 << 30;
        d.memoire.totale = (16 << 30) - (2 << 30);
        let c = conseils(&d);
        assert!(c[0].contains("réservés par le matériel"), "{c:?}");
        assert!(c[1].contains("pilote qui fuit"), "{c:?}");
    }

    #[test]
    fn processes_are_added_up_per_app_and_system_ones_are_not_blamed() {
        let mut d = base();
        d.processus = vec![
            p("chrome.exe", 300),
            p("chrome.exe", 250),
            p("svchost.exe", 900),
            p("notepad.exe", 20),
        ];
        let apps = par_appli(&d.processus);
        assert_eq!(apps[0].nom, "svchost.exe");
        assert_eq!(apps[1].nombre, 2);
        assert_eq!(apps[1].ws, 550 * MO);
        let c = conseils(&d);
        assert!(
            c.iter()
                .any(|l| l.contains("chrome.exe (550 Mo)") && !l.contains("svchost")),
            "{c:?}"
        );
    }

    #[test]
    fn paged_out_private_memory_is_not_blamed_as_ram() {
        let mut d = base();
        d.processus = vec![Processus {
            nom: "Notepad.exe".into(),
            session: 1,
            prive: 612 * MO,
            ws: 5 * MO,
        }];
        assert!(!conseils(&d).iter().any(|l| l.contains("Notepad")));
    }

    #[test]
    fn cache_is_said_not_to_be_lost_memory() {
        let mut d = base();
        d.memoire.cache = 6 << 30;
        assert!(conseils(&d).iter().any(|l| l.contains("pas de la RAM perdue")));
    }

    #[test]
    fn report_text_has_every_section() {
        let mut d = base();
        d.processus = vec![p("ms-teams.exe", 600)];
        d.webviews = vec![("ms-teams.exe".into(), 570 * MO, false)];
        let t = texte(&d);
        for s in [
            "Mémoire",
            "Conseils",
            "Applis",
            "svchost",
            "WebView",
            "démarrage",
            "Prism",
        ] {
            assert!(t.contains(s), "section {s} absente");
        }
        assert!(t.contains("ms-teams.exe"));
    }
}
