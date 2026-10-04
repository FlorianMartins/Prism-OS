//! Logique d'apparence commune aux backends (vrai et simulé).

use std::collections::BTreeMap;

use prism_core::apparence::{apply, current, restore, AppearanceConfig, AppearanceJournal, Catalog};

use crate::backend::{AppearanceRow, PresetInfo};

pub fn rows(sys: &mut dyn AppearanceConfig) -> Vec<AppearanceRow> {
    let c = Catalog::builtin();
    let cur = current(sys, &c);
    c.knobs
        .iter()
        .map(|k| {
            let active = match cur.get(&k.id) {
                Some(Ok(Some(v))) => k.options.iter().position(|o| &o.value == v),
                _ => None,
            };
            AppearanceRow {
                id: k.id.clone(),
                group: k.group.clone(),
                label: k.label.clone(),
                why: k.why.clone(),
                options: k.options.iter().map(|o| o.label.clone()).collect(),
                current: active,
            }
        })
        .collect()
}

pub fn presets() -> Vec<PresetInfo> {
    Catalog::builtin()
        .presets
        .into_iter()
        .map(|p| PresetInfo {
            id: p.id,
            label: p.label,
            description: p.description,
        })
        .collect()
}

fn summary(r: prism_core::apparence::AppearanceReport) -> Result<String, String> {
    if r.failed.is_empty() {
        Ok(format!("{} réglage(s) changé(s)", r.done.len()))
    } else {
        Err(r.failed.join(" · "))
    }
}

pub fn set(
    sys: &mut dyn AppearanceConfig,
    journal: &mut AppearanceJournal,
    id: &str,
    option: usize,
) -> Result<String, String> {
    let c = Catalog::builtin();
    let k = c.knob(id).ok_or("réglage inconnu")?;
    let v = k.options.get(option).ok_or("option inconnue")?.value.clone();
    summary(apply(sys, &c, &BTreeMap::from([(id.to_string(), v)]), journal))
}

pub fn preset(sys: &mut dyn AppearanceConfig, journal: &mut AppearanceJournal, id: &str) -> Result<String, String> {
    let c = Catalog::builtin();
    let p = c.preset(id).ok_or("préréglage inconnu")?;
    summary(apply(sys, &c, &p.values, journal))
}

pub fn restore_all(sys: &mut dyn AppearanceConfig, journal: &mut AppearanceJournal) -> Result<String, String> {
    summary(restore(sys, &Catalog::builtin(), journal))
}
