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
    /// Processeur et carte graphique en %, mesurés comme la Prism Bar.
    pub cpu: Option<f32>,
    pub gpu: Option<f32>,
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
    /// Clé de l'élément (`svc:dps`…) : pour l'appliquer ou le remettre seul.
    pub key: String,
    /// Prism l'a changé (sa valeur d'origine est au journal : on peut la remettre).
    pub by_prism: bool,
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
    /// (identifiant, nom, raison du conflit anti-cheat).
    pub tools: Vec<(String, String, Option<String>)>,
}

/// Réglages Windows d'un jeu (page Jeux).
#[derive(Clone, Debug)]
pub struct GameCfg {
    pub name: String,
    /// Exécutables trouvés dans son dossier (chemins complets).
    pub exes: Vec<String>,
    /// `None` : aucun exécutable trouvé.
    pub gpu: Option<bool>,
    pub plein_ecran: Option<bool>,
}

/// Un service Windows et ce qu'on peut en faire.
#[derive(Clone, Debug)]
pub struct ServiceRow {
    pub info: prism_core::allege::ServiceInfo,
    /// Raison de protection : réglage verrouillé.
    pub protected: Option<String>,
    /// Changé par Prism : on peut remettre l'origine.
    pub by_prism: bool,
    /// Superflu selon Prism : la raison, et le mode qui allège.
    pub superflu: Option<(String, prism_core::allege::StartType)>,
}

/// Installation ou désinstallation d'outils en cours (sans console).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ToolJob {
    pub install: bool,
    /// Outil en cours et rang (1 à `total`).
    pub current: String,
    pub step: usize,
    pub total: usize,
    pub done: bool,
    /// Outils réussis et erreurs, en clair.
    pub ok: Vec<String>,
    pub errors: Vec<String>,
}

/// État de la page Outils cyber.
#[derive(Clone, Debug, Default)]
pub struct ToolsView {
    /// Identifiants des outils installés (connus après une lecture).
    pub installed: Vec<String>,
    /// Lecture de l'état en cours (winget list, wsl -l).
    pub scanning: bool,
    pub scanned: bool,
    pub job: Option<ToolJob>,
}

pub trait Backend {
    /// Droits administrateur (sinon : mode limité, certaines actions échouent).
    fn elevated(&self) -> bool;
    fn relaunch_elevated(&mut self) -> Result<String, String>;
    fn live(&mut self) -> Live;
    fn profiles(&self) -> Vec<ProfileInfo>;
    fn set_profile(&mut self, name: &str) -> Result<String, String>;
    fn start_watch(&mut self) -> Result<String, String>;
    /// Arrête le moteur proprement (tout est rendu à Windows).
    fn stop_watch(&mut self) -> Result<String, String>;
    fn ram_clean(&mut self) -> Result<String, String>;
    /// `prism rapport` : où part la mémoire ; le fichier est ouvert à l'écran.
    fn rapport(&mut self) -> Result<String, String>;

    fn startup(&mut self) -> Result<Vec<StartupRow>, String>;
    fn startup_toggle(&mut self, source: Source, name: &str, on: bool) -> Result<String, String>;
    fn startup_recommended(&mut self) -> Result<String, String>;
    fn startup_restore(&mut self) -> Result<String, String>;

    fn allege(&mut self) -> Vec<AllegeRow>;
    fn allege_apply(&mut self, tier: Tier) -> Result<String, String>;
    fn allege_restore(&mut self) -> Result<String, String>;
    /// Applique (`on`) ou remet (`!on`) un seul élément.
    fn allege_toggle(&mut self, key: &str, on: bool) -> Result<String, String>;
    /// Compression de la mémoire de Windows (`None` : illisible).
    fn compression(&mut self) -> Option<bool> {
        None
    }
    fn set_compression(&mut self, _on: bool) -> Result<String, String> {
        Err("non disponible".into())
    }

    /// Réglages par jeu (lit les dossiers des jeux : à appeler à l'ouverture de la page).
    fn game_cfgs(&mut self) -> Vec<GameCfg>;
    fn game_set(&mut self, name: &str, r: prism_core::jeux::Reglage, on: bool) -> Result<String, String>;
    fn last_session(&mut self) -> Option<prism_core::jeux::Partie>;

    /// Tous les services Windows (page Services).
    fn services(&mut self) -> Result<Vec<ServiceRow>, String>;
    fn service_set(&mut self, name: &str, to: prism_core::allege::StartType) -> Result<String, String>;
    /// Remet le mode de démarrage d'origine (si Prism l'a changé).
    fn service_restore(&mut self, name: &str) -> Result<String, String>;
    /// Tous les services superflus d'un coup (protégés exclus), réversible.
    fn services_slim(&mut self) -> Result<String, String> {
        let rows = self.services()?;
        let (mut done, mut failed) = (0, Vec::new());
        for r in rows.iter().filter(|r| r.protected.is_none() && !r.info.per_user) {
            let Some((_, to)) = &r.superflu else { continue };
            if r.info.start == Some(*to) || r.info.start == Some(prism_core::allege::StartType::Disabled) {
                continue;
            }
            match self.service_set(&r.info.name, *to) {
                Ok(_) => done += 1,
                Err(e) => failed.push(e),
            }
        }
        if failed.is_empty() {
            Ok(format!(
                "{done} service(s) superflu(s) allégé(s) — réversible (↺ ou « Tout restaurer »)"
            ))
        } else {
            Err(format!(
                "{done} allégé(s), {} échec(s) : {}",
                failed.len(),
                failed.join(" ; ")
            ))
        }
    }

    /// Plan automatique des jeux à anti-cheat noyau (réglages de l'utilisateur).
    fn noyau(&mut self) -> prism_core::noyau::Reglages {
        prism_core::noyau::Reglages::charger(&prism_core::paths::user_dir())
    }
    /// Nettoyage automatique de la RAM (réglages de l'utilisateur, lus par le moteur).
    fn ram_auto(&mut self) -> prism_core::ram_auto::Reglages {
        prism_core::ram_auto::Reglages::charger(&prism_core::paths::user_dir())
    }
    fn set_ram_auto(&mut self, r: &prism_core::ram_auto::Reglages) -> Result<String, String> {
        r.enregistrer(&prism_core::paths::user_dir())?;
        Ok(if r.actif {
            format!(
                "Nettoyage auto : toutes les {} min{}",
                r.toutes_les_minutes,
                if r.seuil_pourcent > 0 {
                    format!(" et au-delà de {} % utilisés", r.seuil_pourcent)
                } else {
                    String::new()
                }
            )
        } else {
            "Nettoyage automatique désactivé".into()
        })
    }
    /// Fermeture des WebView des applis restées sans fenêtre.
    fn webview(&mut self) -> prism_core::webview::Reglages {
        prism_core::webview::Reglages::charger(&prism_core::paths::user_dir())
    }
    fn set_webview(&mut self, r: &prism_core::webview::Reglages) -> Result<String, String> {
        r.enregistrer(&prism_core::paths::user_dir())?;
        Ok("WebView en arrière-plan : réglage enregistré".into())
    }
    fn set_noyau(&mut self, r: &prism_core::noyau::Reglages) -> Result<String, String> {
        r.enregistrer(&prism_core::paths::user_dir())?;
        Ok("Plan « jeu noyau » enregistré : appliqué à la prochaine partie".into())
    }

    /// Tableau de bord vie privée et connexions ouvertes par la télémétrie.
    fn privacy(
        &mut self,
    ) -> (
        Vec<prism_core::privacy::Row>,
        Vec<prism_core::privacy::TelemetryConnection>,
    );
    fn privacy_apply(&mut self, level: prism_core::privacy::Level) -> Result<String, String>;
    fn privacy_restore(&mut self) -> Result<String, String>;
    /// Applique (`on`) ou remet (`!on`) un seul réglage ou une seule règle.
    fn privacy_toggle(&mut self, key: &str, on: bool) -> Result<String, String>;

    fn games(&mut self) -> Vec<Game>;
    fn launch(&mut self, game: &Game) -> Result<String, String>;

    fn appearance(&mut self) -> Vec<AppearanceRow>;
    fn appearance_presets(&self) -> Vec<PresetInfo>;
    fn appearance_set(&mut self, id: &str, option: usize) -> Result<String, String>;
    fn appearance_preset(&mut self, id: &str) -> Result<String, String>;
    fn appearance_restore(&mut self) -> Result<String, String>;
    /// Thème de Prism appliqué à tout Windows : couleur d'accent, fond d'écran généré,
    /// mode sombre, barres de titre colorées (chacun au choix). Réversible.
    fn windows_design_apply(&mut self, accent: bool, fond: bool, sombre: bool, titres: bool) -> Result<String, String> {
        let mut done = Vec::new();
        if sombre {
            self.appearance_set("apps_theme", 0)?;
            self.appearance_set("system_theme", 0)?;
            done.push("mode sombre".to_string());
        }
        if titres {
            self.appearance_set("accent_titlebars", 0)?;
            done.push("barres de titre colorées".to_string());
        }
        done.extend(self.windows_design_accent_fond(accent, fond)?);
        Ok(if done.is_empty() {
            "Rien à appliquer".into()
        } else {
            format!("Windows : {}", done.join(", "))
        })
    }
    /// Accent et fond d'écran (propre à la plateforme).
    fn windows_design_accent_fond(&mut self, _accent: bool, _fond: bool) -> Result<Vec<String>, String> {
        Ok(Vec::new())
    }
    /// Remet le design de Windows d'avant Prism (accent, fond, réglages d'apparence).
    fn windows_design_restore(&mut self) -> Result<String, String> {
        self.appearance_restore()
    }

    fn bar_config(&mut self) -> prism_core::bar::BarConfig;
    fn set_bar_config(&mut self, cfg: &prism_core::bar::BarConfig) -> Result<(), String>;
    fn bar_running(&mut self) -> bool;
    fn bar_start(&mut self) -> Result<String, String>;
    /// L'accueil du premier lancement a-t-il déjà été vu ?
    /// Prism (moteur + barre) se lance-t-il à l'ouverture de session ?
    fn autostart(&mut self) -> bool;
    fn set_autostart(&mut self, on: bool) -> Result<String, String>;
    /// Lance la désinstallation de Windows (qui demande confirmation).
    fn uninstall(&mut self) -> Result<String, String>;
    fn welcome_done(&mut self) -> bool;
    fn set_welcome_done(&mut self);
    fn bar_stop(&mut self) -> Result<String, String>;
    /// Arrête la barre sans toucher à son démarrage automatique (« Désactiver Prism »).
    fn bar_pause(&mut self) -> Result<String, String>;

    fn packs(&self) -> Vec<PackInfo>;
    fn install_pack(&mut self, pack: &str) -> Result<String, String>;
    /// Lit quels outils sont installés (en arrière-plan).
    fn tools_scan(&mut self);
    fn tools_view(&mut self) -> ToolsView;
    /// Installe (avec leurs dépendances) ou désinstalle ces outils, sans console.
    fn tools_run(&mut self, install: bool, ids: Vec<String>) -> Result<String, String>;
}
