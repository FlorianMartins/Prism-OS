//! Mise en forme lisible des rapports pour la console.

use prism_core::engine::Report;
use prism_core::model::{human_bytes, MemStatus};
use prism_core::tools::Conflict;

pub fn memory(m: &MemStatus) -> String {
    let mut s = format!(
        "RAM : {} libre sur {} ({} %)",
        human_bytes(m.free),
        human_bytes(m.total),
        m.free_percent()
    );
    if m.standby_total > 0 {
        s.push_str(&format!(
            " · cache {} dont {} en priorité basse",
            human_bytes(m.standby_total),
            human_bytes(m.standby_low)
        ));
    }
    s
}

/// Résumé : les actions faites sont regroupées, les problèmes listés un par un.
pub fn report(r: &Report, indent: &str) {
    let count = |needle: &str| r.done.iter().filter(|d| d.contains(needle)).count();
    let lines = [
        ("priorité CPU", "processus passés en retrait"),
        ("EcoQoS", "processus en EcoQoS (cœurs économes)"),
        ("priorité mémoire", "priorités mémoire changées"),
        ("rognée", "mémoires de travail rognées"),
    ];
    for (needle, label) in lines {
        let n = count(needle);
        if n > 0 {
            println!("{indent}{n} {label}");
        }
    }
    for d in r.done.iter().filter(|d| !lines.iter().any(|(n, _)| d.contains(n))) {
        println!("{indent}{d}");
    }
    if let (Some(a), Some(b)) = (r.mem_before, r.mem_after) {
        if b.free > a.free {
            println!("{indent}RAM libérée : {}", human_bytes(b.free - a.free));
        }
    }
    if !r.skipped.is_empty() {
        println!(
            "{indent}{} ignoré(s) (processus fermé, droits, déjà bas)",
            r.skipped.len()
        );
    }
    for f in &r.failed {
        println!("{indent}ÉCHEC : {f}");
    }
}

pub fn conflicts(c: &[Conflict]) {
    for x in c {
        println!("  ⚠ {} ouvert ({}) : {}", x.tool, x.process, x.reason);
    }
}

/// Compte rendu d'une restauration.
pub fn restored(r: &Report, indent: &str) {
    println!("{indent}{} réglage(s) remis à leur valeur d'origine", r.done.len());
    if !r.skipped.is_empty() {
        println!("{indent}{} ignoré(s) (processus fermés entre-temps)", r.skipped.len());
    }
    for f in &r.failed {
        println!("{indent}ÉCHEC : {f}");
    }
}
