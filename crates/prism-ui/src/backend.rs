//! Contrat entre l'interface et le système. L'interface ne parle qu'à ce trait :
//! sous Windows il appelle les vraies fonctions, ailleurs une simulation (tests,
//! captures d'écran).

use prism_core::allege::Tier;
use prism_core::demarrage::Source;
use prism_core::etat::Etat;
use prism_core::library::Game;
use prism_core::model::MemStatus;

#[derive(Clone, Debug, Default)]
pub struct TopProc {
    pub name: String,
    pub cpu_percent: f32,
    pub ram: u64,
    pub class: String,
}

#[derive(Clone, Debug, Default)]
pub struct Live {
    pub profile: String,
    pub mem: MemStatus,
    pub top: Vec<TopProc>,
    pub cores: String,
    /// État écrit par `prism watch` (None : jamais lancé).
    pub etat: Option<Etat>,
    /// `prism watch` tourne-t-il en ce moment ?
    pub watch_alive: bool,
}

#[derive(Clone, Debug)]
pub struct ProfileInfo {
    pub name: String,
    pub label: String,
    pub description: String,
}

#[derive(Clone, Debug)]
pub struct StartupRow {
    pub source: Source,
    pub name: String,
    pub enabled: bool,
    /// « à désactiver », « optionnel », « à garder », « protégé » ou « inconnu ».
    pub advice: String,
    pub why: String,
    pub protected: bool,
}

#[derive(Clone, Debug)]
pub struct AllegeRow {
    pub tier: Tier,
    pub label: String,
    pub current: String,
    pub target: String,
    pub done: bool,
    pub why: String,
}

#[derive(Clone, Debug)]
pub struct AppearanceRow {
    pub id: String,
    pub group: String,
    pub label: String,
    pub why: String,
    pub options: Vec<String>,
    /// Option active (None : valeur absente ou hors des options proposées).
    pub current: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct PresetInfo {
    pub id: String,
    pub label: String,
    pub description: String,
}

#[derive(Clone, Debug)]
pub struct PackInfo {
    pub id: String,
    pub label: String,
    pub tools: Vec<(String, Option<String>)>,
}

pub trait Backend {
    /// Droits administrateur (sinon : mode limité, certaines actions échouent).
    fn elevated(&self) -> bool;
    fn relaunch_elevated(&mut self) -> Result<String, String>;
    fn live(&mut self) -> Live;
    fn profiles(&self) -> Vec<ProfileInfo>;
    fn set_profile(&mut self, name: &str) -> Result<String, String>;
    fn start_watch(&mut self) -> Result<String, String>;
    fn ram_clean(&mut self) -> Result<String, String>;

    fn startup(&mut self) -> Result<Vec<StartupRow>, String>;
    fn startup_toggle(&mut self, source: Source, name: &str, on: bool) -> Result<String, String>;
    fn startup_recommended(&mut self) -> Result<String, String>;
    fn startup_restore(&mut self) -> Result<String, String>;

    fn allege(&mut self) -> Vec<AllegeRow>;
    fn allege_apply(&mut self, tier: Tier) -> Result<String, String>;
    fn allege_restore(&mut self) -> Result<String, String>;

    fn games(&mut self) -> Vec<Game>;
    fn launch(&mut self, game: &Game) -> Result<String, String>;

    fn appearance(&mut self) -> Vec<AppearanceRow>;
    fn appearance_presets(&self) -> Vec<PresetInfo>;
    fn appearance_set(&mut self, id: &str, option: usize) -> Result<String, String>;
    fn appearance_preset(&mut self, id: &str) -> Result<String, String>;
    fn appearance_restore(&mut self) -> Result<String, String>;

    fn bar_config(&mut self) -> prism_core::bar::BarConfig;
    fn set_bar_config(&mut self, cfg: &prism_core::bar::BarConfig) -> Result<(), String>;
    fn bar_running(&mut self) -> bool;
    fn bar_start(&mut self) -> Result<String, String>;
    fn bar_stop(&mut self) -> Result<String, String>;

    fn packs(&self) -> Vec<PackInfo>;
    fn install_pack(&mut self, pack: &str) -> Result<String, String>;
}
