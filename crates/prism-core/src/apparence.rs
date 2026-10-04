//! Apparence de Windows : animations, effets, thème, réactivité. Uniquement des
//! réglages officiels, chacun réversible (journal des valeurs d'origine).

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::allege::{Hive, RegData};

pub const APPARENCE_TOML: &str = include_str!("../../../config/apparence.toml");

/// Clés de registre que l'Apparence peut écrire (chemin complet, minuscules).
pub const APPEARANCE_ALLOWLIST: [&str; 4] = [
    "hkcu\\software\\microsoft\\windows\\currentversion\\themes\\personalize",
    "hkcu\\software\\microsoft\\windows\\currentversion\\explorer\\advanced",
    "hkcu\\control panel\\desktop",
    "hkcu\\software\\microsoft\\windows\\dwm",
];

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpiDef {
    pub get: u32,
    pub set: u32,
    /// La valeur passe par `uiParam` (sinon par `pvParam`).
    #[serde(default)]
    pub ui: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegDef {
    pub hive: Hive,
    pub key: String,
    pub value: String,
}

impl RegDef {
    pub fn full_key(&self) -> String {
        format!("{}\\{}", self.hive.prefix(), self.key)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Setting {
    /// Animation réduire/agrandir (`SPI_GETANIMATION` / `SPI_SETANIMATION`).
    MinMax(MinMaxTag),
    Spi(SpiSetting),
    Reg(RegSetting),
}

// Une structure stricte par variante : une énumération « untagged » ignorerait sinon
// les clés en trop (faute de frappe muette dans le catalogue).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpiSetting {
    pub spi: SpiDef,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegSetting {
    pub reg: RegDef,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MinMaxTag {
    Minmax,
}

/// Valeur d'un réglage : booléen (SPI) ou donnée de registre.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum KnobValue {
    Bool(bool),
    Dword(u32),
    Text(String),
}

impl KnobValue {
    pub fn to_reg(&self) -> Option<RegData> {
        match self {
            KnobValue::Dword(d) => Some(RegData::Dword(*d)),
            KnobValue::Text(t) => Some(RegData::Text(t.clone())),
            KnobValue::Bool(_) => None,
        }
    }

    pub fn from_reg(d: &RegData) -> KnobValue {
        match d {
            RegData::Dword(v) => KnobValue::Dword(*v),
            RegData::Text(t) => KnobValue::Text(t.clone()),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnobOption {
    pub label: String,
    pub value: KnobValue,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Knob {
    pub id: String,
    pub group: String,
    pub label: String,
    pub why: String,
    pub setting: Setting,
    pub options: Vec<KnobOption>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preset {
    pub id: String,
    pub label: String,
    pub description: String,
    pub values: BTreeMap<String, KnobValue>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub knobs: Vec<Knob>,
    #[serde(default)]
    pub presets: Vec<Preset>,
}

fn appearance_allowed(full_key: &str) -> bool {
    let k = full_key.to_ascii_lowercase();
    APPEARANCE_ALLOWLIST.iter().any(|a| k == *a)
}

impl Catalog {
    pub fn parse(text: &str) -> Result<Catalog, String> {
        let c: Catalog = toml::from_str(text).map_err(|e| format!("catalogue d'apparence illisible : {e}"))?;
        c.validate()?;
        Ok(c)
    }

    pub fn builtin() -> Catalog {
        Catalog::parse(APPARENCE_TOML).expect("catalogue embarqué validé par les tests")
    }

    pub fn knob(&self, id: &str) -> Option<&Knob> {
        self.knobs.iter().find(|k| k.id == id)
    }

    pub fn preset(&self, id: &str) -> Option<&Preset> {
        self.presets.iter().find(|p| p.id == id)
    }

    /// Groupes dans l'ordre du fichier.
    pub fn groups(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for k in &self.knobs {
            if !out.contains(&k.group) {
                out.push(k.group.clone());
            }
        }
        out
    }

    pub fn validate(&self) -> Result<(), String> {
        let mut errors = Vec::new();
        let mut ids = HashSet::new();
        for k in &self.knobs {
            if !ids.insert(k.id.as_str()) {
                errors.push(format!("réglage en double : {}", k.id));
            }
            if k.options.len() < 2 {
                errors.push(format!("{} : au moins deux options", k.id));
            }
            let bool_setting = !matches!(k.setting, Setting::Reg(_));
            for o in &k.options {
                let is_bool = matches!(o.value, KnobValue::Bool(_));
                if is_bool != bool_setting {
                    errors.push(format!("{} : l'option « {} » n'a pas le bon type", k.id, o.label));
                }
            }
            if let Setting::Reg(RegSetting { reg }) = &k.setting {
                if !appearance_allowed(&reg.full_key()) {
                    errors.push(format!("{} : clé {} hors de la liste autorisée", k.id, reg.full_key()));
                }
            }
        }
        for p in &self.presets {
            for (id, v) in &p.values {
                match self.knob(id) {
                    None => errors.push(format!("préréglage {} : réglage inconnu « {id} »", p.id)),
                    Some(k) if !k.options.iter().any(|o| &o.value == v) => errors.push(format!(
                        "préréglage {} : valeur {v:?} absente des options de {id}",
                        p.id
                    )),
                    Some(_) => {}
                }
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("\n"))
        }
    }
}

/// Lecture / écriture des réglages (Windows, ou simulation).
pub trait AppearanceConfig {
    /// `Ok(None)` : réglage absent (valeur de registre non définie).
    fn read(&mut self, s: &Setting) -> Result<Option<KnobValue>, String>;
    /// `None` : supprime la valeur de registre (retour à « non défini »).
    fn write(&mut self, s: &Setting, v: Option<&KnobValue>) -> Result<(), String>;
    /// Prévient l'Explorateur et les applis que l'apparence a changé.
    fn notify(&mut self) {}
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Original {
    pub id: String,
    pub was: Option<KnobValue>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppearanceJournal {
    pub originals: Vec<Original>,
}

impl AppearanceJournal {
    pub fn load(path: &PathBuf) -> Result<AppearanceJournal, String> {
        match fs::read(path) {
            Ok(b) => serde_json::from_slice(&b).map_err(|e| format!("journal d'apparence illisible : {e}")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(AppearanceJournal::default()),
            Err(e) => Err(format!("{} : {e}", path.display())),
        }
    }

    pub fn save(&self, path: &PathBuf) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        fs::rename(&tmp, path).map_err(|e| e.to_string())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppearanceReport {
    pub done: Vec<String>,
    pub unchanged: Vec<String>,
    pub failed: Vec<String>,
}

/// Valeur actuelle de chaque réglage (pour l'affichage).
pub fn current(
    sys: &mut dyn AppearanceConfig,
    catalog: &Catalog,
) -> BTreeMap<String, Result<Option<KnobValue>, String>> {
    catalog
        .knobs
        .iter()
        .map(|k| (k.id.clone(), sys.read(&k.setting)))
        .collect()
}

/// Applique des valeurs (un préréglage ou des choix individuels). La valeur
/// d'origine de chaque réglage est journalisée une seule fois.
pub fn apply(
    sys: &mut dyn AppearanceConfig,
    catalog: &Catalog,
    values: &BTreeMap<String, KnobValue>,
    journal: &mut AppearanceJournal,
) -> AppearanceReport {
    let mut r = AppearanceReport::default();
    for (id, v) in values {
        let Some(k) = catalog.knob(id) else {
            r.failed.push(format!("{id} : réglage inconnu"));
            continue;
        };
        if !k.options.iter().any(|o| &o.value == v) {
            r.failed.push(format!("{} : valeur non proposée", k.label));
            continue;
        }
        match sys.read(&k.setting) {
            Err(e) => r.failed.push(format!("{} — {e}", k.label)),
            Ok(cur) if cur.as_ref() == Some(v) => r.unchanged.push(k.label.clone()),
            Ok(cur) => match sys.write(&k.setting, Some(v)) {
                Ok(()) => {
                    if !journal.originals.iter().any(|o| o.id == k.id) {
                        journal.originals.push(Original {
                            id: k.id.clone(),
                            was: cur,
                        });
                    }
                    r.done.push(k.label.clone());
                }
                Err(e) => r.failed.push(format!("{} — {e}", k.label)),
            },
        }
    }
    if !r.done.is_empty() {
        sys.notify();
    }
    r
}

/// Remet toutes les valeurs d'origine ; le journal ne garde que les échecs.
pub fn restore(sys: &mut dyn AppearanceConfig, catalog: &Catalog, journal: &mut AppearanceJournal) -> AppearanceReport {
    let mut r = AppearanceReport::default();
    let mut kept = Vec::new();
    for o in journal.originals.iter().rev() {
        let Some(k) = catalog.knob(&o.id) else {
            kept.push(o.clone());
            continue;
        };
        match sys.write(&k.setting, o.was.as_ref()) {
            Ok(()) => r.done.push(k.label.clone()),
            Err(e) => {
                r.failed.push(format!("{} — {e}", k.label));
                kept.push(o.clone());
            }
        }
    }
    kept.reverse();
    journal.originals = kept;
    if !r.done.is_empty() {
        sys.notify();
    }
    r
}

/// Simulation : un Windows 11 d'usine.
#[derive(Clone, Debug)]
pub struct MockAppearance {
    pub values: BTreeMap<String, KnobValue>,
    pub notified: usize,
}

fn setting_key(s: &Setting) -> String {
    match s {
        Setting::MinMax(_) => "minmax".into(),
        Setting::Spi(SpiSetting { spi }) => format!("spi:{:#x}", spi.get),
        Setting::Reg(RegSetting { reg }) => {
            format!("reg:{}\\{}", reg.full_key().to_lowercase(), reg.value.to_lowercase())
        }
    }
}

impl MockAppearance {
    pub fn factory(catalog: &Catalog) -> MockAppearance {
        let mut values = BTreeMap::new();
        for k in &catalog.knobs {
            let v = match &k.setting {
                Setting::Reg(RegSetting { reg }) if reg.value == "MenuShowDelay" => Some(KnobValue::Text("400".into())),
                Setting::Reg(RegSetting { reg }) if reg.value == "ColorPrevalence" => None,
                Setting::Reg(RegSetting { reg }) if reg.value == "TaskbarAl" => None,
                Setting::Reg(_) => Some(KnobValue::Dword(1)),
                _ => Some(KnobValue::Bool(true)),
            };
            if let Some(v) = v {
                values.insert(setting_key(&k.setting), v);
            }
        }
        MockAppearance { values, notified: 0 }
    }
}

impl AppearanceConfig for MockAppearance {
    fn read(&mut self, s: &Setting) -> Result<Option<KnobValue>, String> {
        Ok(self.values.get(&setting_key(s)).cloned())
    }
    fn write(&mut self, s: &Setting, v: Option<&KnobValue>) -> Result<(), String> {
        match v {
            Some(v) => self.values.insert(setting_key(s), v.clone()),
            None => self.values.remove(&setting_key(s)),
        };
        Ok(())
    }
    fn notify(&mut self) {
        self.notified += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_catalog_is_valid() {
        let c = Catalog::builtin();
        assert!(c.knobs.len() >= 18);
        assert_eq!(c.groups(), ["Animations", "Effets", "Thème", "Réactivité"]);
        assert!(c.preset("performance").is_some() && c.preset("fluide").is_some());
    }

    #[test]
    fn performance_preset_turns_animations_off_and_restore_is_exact() {
        let c = Catalog::builtin();
        let mut m = MockAppearance::factory(&c);
        let factory = m.values.clone();
        let mut j = AppearanceJournal::default();
        let r = apply(&mut m, &c, &c.preset("performance").unwrap().values, &mut j);
        assert!(r.failed.is_empty(), "{:?}", r.failed);
        let minmax = c.knob("anim_minmax").unwrap();
        assert_eq!(m.read(&minmax.setting).unwrap(), Some(KnobValue::Bool(false)));
        let font = c.knob("font_smoothing").unwrap();
        assert_eq!(
            m.read(&font.setting).unwrap(),
            Some(KnobValue::Bool(true)),
            "le lissage reste"
        );
        assert!(r.unchanged.iter().any(|u| u.contains("Lissage")));
        assert_eq!(m.notified, 1, "l'Explorateur est prévenu une fois");
        restore(&mut m, &c, &mut j);
        assert_eq!(m.values, factory);
        assert!(j.originals.is_empty());
    }

    #[test]
    fn switching_presets_keeps_the_factory_value_as_original() {
        let c = Catalog::builtin();
        let mut m = MockAppearance::factory(&c);
        let factory = m.values.clone();
        let mut j = AppearanceJournal::default();
        apply(&mut m, &c, &c.preset("performance").unwrap().values, &mut j);
        apply(&mut m, &c, &c.preset("fluide").unwrap().values, &mut j);
        let delay = c.knob("menu_delay").unwrap();
        assert_eq!(m.read(&delay.setting).unwrap(), Some(KnobValue::Text("100".into())));
        restore(&mut m, &c, &mut j);
        assert_eq!(m.values, factory, "on revient à l'usine, pas au préréglage précédent");
    }

    #[test]
    fn an_absent_registry_value_is_deleted_again_on_restore() {
        let c = Catalog::builtin();
        let mut m = MockAppearance::factory(&c);
        let align = c.knob("taskbar_align").unwrap();
        assert_eq!(m.read(&align.setting).unwrap(), None);
        let mut j = AppearanceJournal::default();
        let values = BTreeMap::from([("taskbar_align".to_string(), KnobValue::Dword(0))]);
        apply(&mut m, &c, &values, &mut j);
        assert_eq!(m.read(&align.setting).unwrap(), Some(KnobValue::Dword(0)));
        restore(&mut m, &c, &mut j);
        assert_eq!(m.read(&align.setting).unwrap(), None);
    }

    #[test]
    fn values_outside_the_options_are_refused() {
        let c = Catalog::builtin();
        let mut m = MockAppearance::factory(&c);
        let mut j = AppearanceJournal::default();
        let values = BTreeMap::from([("menu_delay".to_string(), KnobValue::Text("5000".into()))]);
        let r = apply(&mut m, &c, &values, &mut j);
        assert_eq!(r.failed.len(), 1);
        assert!(j.originals.is_empty());
    }

    #[test]
    fn invalid_catalogs_are_rejected() {
        let bad = APPARENCE_TOML.replace(
            "value = \"EnableTransparency\"",
            "value = \"EnableTransparency\" }, extra = { a = 1",
        );
        assert!(Catalog::parse(&bad).is_err());
        let bad = APPARENCE_TOML.replacen(
            r"key = 'Control Panel\Desktop'",
            r"key = 'Software\Microsoft\Windows\CurrentVersion\Run'",
            1,
        );
        assert!(Catalog::parse(&bad).unwrap_err().contains("hors de la liste"));
        let bad = APPARENCE_TOML.replace("menu_delay = \"0\"", "menu_delay = \"7\"");
        assert!(Catalog::parse(&bad).unwrap_err().contains("absente des options"));
        let bad = APPARENCE_TOML.replacen(
            "{ label = \"Animée\", value = true }",
            "{ label = \"Animée\", value = 1 }",
            1,
        );
        assert!(Catalog::parse(&bad).unwrap_err().contains("bon type"));
    }
}
