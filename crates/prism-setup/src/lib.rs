//! Programme d'installation de Prism OS : un logo de prisme 3D animé et trois écrans
//! (bienvenue et choix du dossier, installation séquencée, fin). L'installation
//! elle-même est faite par le paquet MSI embarqué (désinstallation et mises à jour
//! restent celles de Windows) ; l'interface ne parle qu'au trait `Installer`, ce qui
//! permet de la tester sans Windows.

use std::sync::{Arc, Mutex};

use eframe::egui::{self, RichText, Sense, Vec2};
use prism_ui::logo::draw_prism;
use prism_ui::theme as th;

/// Étapes montrées pendant l'installation, dans l'ordre.
pub const STEPS: [&str; 4] = [
    "Préparation",
    "Copie des fichiers",
    "Configuration de Windows (PATH, raccourcis)",
    "Finalisation",
];

/// Avancement d'une installation (partagé avec le fil qui installe).
#[derive(Clone, Debug, Default)]
pub struct Progress {
    /// 0 à 1.
    pub fraction: f32,
    /// Étape en cours (index dans `STEPS`).
    pub step: usize,
    /// Fin : `Ok` ou le message d'erreur.
    pub done: Option<Result<(), String>>,
}

pub type Shared = Arc<Mutex<Progress>>;

/// Ce que l'installateur sait faire (Windows, ou simulation pour les tests).
pub trait Installer {
    /// Dossier proposé par défaut (celui de l'installation existante s'il y en a une).
    fn default_folder(&self) -> String;
    /// Version déjà installée, s'il y en a une.
    fn installed_version(&self) -> Option<String>;
    /// Ouvre le sélecteur de dossier de Windows.
    fn pick_folder(&self, current: &str) -> Option<String>;
    /// Lance l'installation en arrière-plan ; l'avancement arrive dans `progress`.
    fn start(&mut self, folder: &str, progress: Shared);
    /// Ouvre Prism (sans droits administrateur).
    fn launch(&self, folder: &str);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Welcome,
    Installing,
    Finished,
}

pub struct SetupApp {
    installer: Box<dyn Installer>,
    pub screen: Screen,
    folder: String,
    progress: Shared,
    launch: bool,
    /// Heure (secondes egui) du début de l'écran, pour les animations.
    since: Option<f64>,
    /// Angle du logo, intégré image par image (la vitesse change selon l'écran).
    angle: f32,
    last_time: Option<f64>,
    themed: bool,
    /// Tests : temps figé (captures reproductibles).
    pub frozen_time: Option<f64>,
    /// Mode automatique (`--auto <dossier>`) : installe sans attendre de clic et se
    /// ferme deux secondes après la fin, sans ouvrir Prism (essais, captures).
    pub auto: Option<String>,
}

impl SetupApp {
    pub fn new(installer: Box<dyn Installer>) -> SetupApp {
        let folder = installer.default_folder();
        SetupApp {
            installer,
            screen: Screen::Welcome,
            folder,
            progress: Arc::new(Mutex::new(Progress::default())),
            launch: true,
            since: None,
            angle: 0.6,
            last_time: None,
            themed: false,
            frozen_time: None,
            auto: None,
        }
    }

    /// Accès direct à l'avancement (tests).
    pub fn progress(&self) -> Shared {
        self.progress.clone()
    }

    fn go(&mut self, s: Screen) {
        self.screen = s;
        self.since = None;
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if !self.themed {
            th::apply(&ctx);
            self.themed = true;
        }
        let now = self.frozen_time.unwrap_or_else(|| ctx.input(|i| i.time));
        // Temps figé (tests) : compté depuis 0, la séquence d'apparition est terminée.
        let since = *self
            .since
            .get_or_insert(if self.frozen_time.is_some() { 0.0 } else { now });
        let t = (now - since) as f32;
        let dt = self.last_time.map_or(0.0, |l| (now - l) as f32).clamp(0.0, 0.1);
        self.last_time = Some(now);

        if let (Some(folder), Screen::Welcome) = (self.auto.clone(), self.screen) {
            if t > 2.0 {
                self.folder = folder;
                self.installer.start(&self.folder, self.progress.clone());
                self.go(Screen::Installing);
            }
        }
        if self.auto.is_some() && self.screen == Screen::Finished && t > 2.5 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        let p = self.progress.lock().map(|p| p.clone()).unwrap_or_default();
        if self.screen == Screen::Installing && matches!(p.done, Some(Ok(()))) {
            self.go(Screen::Finished);
        }
        // Vitesse de rotation : calme à l'accueil, rapide pendant l'installation,
        // ralentit jusqu'à l'arrêt à la fin.
        let speed = match self.screen {
            Screen::Welcome => 0.6,
            Screen::Installing => 1.6 + 1.2 * p.fraction,
            Screen::Finished => 1.6 * (1.0 - (t / 1.6).min(1.0)).powi(2),
        };
        self.angle += speed * dt;

        egui::Frame::new()
            .fill(th::bg())
            .inner_margin(egui::Margin::symmetric(36, 24))
            .show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                ui.vertical_centered(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(220.0, 200.0), Sense::hover());
                    let build = match self.screen {
                        Screen::Welcome => t,
                        _ => 10.0,
                    };
                    draw_prism(
                        ui.painter(),
                        rect.center(),
                        74.0,
                        self.angle,
                        build,
                        self.screen == Screen::Finished,
                        t,
                    );
                    ui.add_space(4.0);
                    ui.label(RichText::new("Prism OS").size(30.0).strong().color(th::text()));
                    ui.label(
                        RichText::new("La couche gaming et cybersécurité pour votre Windows · Hivey")
                            .color(th::muted()),
                    );
                    ui.add_space(18.0);
                    match self.screen {
                        Screen::Welcome => self.welcome(ui),
                        Screen::Installing => self.installing(ui, &p, t),
                        Screen::Finished => self.finished(ui, &ctx),
                    }
                });
            });
        ctx.request_repaint();
    }

    fn welcome(&mut self, ui: &mut egui::Ui) {
        if let Some(v) = self.installer.installed_version() {
            ui.label(
                RichText::new(format!(
                    "Prism {v} est déjà installé : il sera mis à jour en {}, vos réglages sont gardés.",
                    env!("CARGO_PKG_VERSION")
                ))
                .color(th::accent()),
            );
            ui.add_space(8.0);
        }
        ui.label(RichText::new("Dossier d'installation").color(th::muted()));
        ui.horizontal(|ui| {
            ui.add_space((ui.available_width() - 520.0).max(0.0) / 2.0);
            ui.add(egui::TextEdit::singleline(&mut self.folder).desired_width(400.0));
            if ui.button("Parcourir…").clicked() {
                if let Some(f) = self.installer.pick_folder(&self.folder) {
                    self.folder = f;
                }
            }
        });
        ui.add_space(6.0);
        ui.label(
            RichText::new("Rien n'est modifié dans Windows sans votre accord ; désinstaller remet tout comme avant.")
                .small()
                .color(th::muted()),
        );
        ui.add_space(18.0);
        let go = egui::Button::new(RichText::new("Installer").size(17.0).strong().color(th::on_accent()))
            .fill(th::accent())
            .min_size(Vec2::new(200.0, 42.0));
        let ready = !self.folder.trim().is_empty();
        if ui.add_enabled(ready, go).clicked() {
            if let Ok(mut p) = self.progress.lock() {
                *p = Progress::default();
            }
            self.installer.start(self.folder.trim(), self.progress.clone());
            self.go(Screen::Installing);
        }
    }

    fn installing(&mut self, ui: &mut egui::Ui, p: &Progress, t: f32) {
        if let Some(Err(e)) = &p.done {
            ui.label(
                RichText::new("L'installation n'a pas abouti")
                    .size(18.0)
                    .color(th::bad()),
            );
            ui.label(RichText::new(e).color(th::text()));
            ui.add_space(12.0);
            if ui.button("Réessayer").clicked() {
                self.go(Screen::Welcome);
            }
            return;
        }
        ui.add(
            egui::ProgressBar::new(p.fraction)
                .desired_width(460.0)
                .fill(th::accent())
                .show_percentage(),
        );
        ui.add_space(14.0);
        // Étapes séquencées : cochées, en cours (point animé), à venir.
        for (i, step) in STEPS.iter().enumerate() {
            let (mark, color) = if i < p.step {
                ("✔".to_string(), th::ok())
            } else if i == p.step {
                let dots = ((t * 3.0) as usize % 3) + 1;
                (format!("{:<3}", ".".repeat(dots)), th::accent())
            } else {
                ("·".to_string(), th::muted())
            };
            ui.horizontal(|ui| {
                ui.add_space((ui.available_width() - 420.0).max(0.0) / 2.0);
                ui.label(RichText::new(mark).monospace().color(color));
                ui.label(RichText::new(*step).color(if i <= p.step { th::text() } else { th::muted() }));
            });
        }
    }

    fn finished(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.label(RichText::new("Prism est installé").size(20.0).strong().color(th::ok()));
        ui.label(RichText::new(format!("dans {}", self.folder)).color(th::muted()));
        ui.add_space(4.0);
        ui.label(
            RichText::new("Menu Démarrer : « Prism » pour l'ouvrir, « Désinstaller Prism » pour tout retirer.")
                .small()
                .color(th::muted()),
        );
        ui.add_space(14.0);
        ui.checkbox(&mut self.launch, "Lancer Prism");
        ui.add_space(10.0);
        let end = egui::Button::new(RichText::new("Terminer").size(17.0).strong().color(th::on_accent()))
            .fill(th::accent())
            .min_size(Vec2::new(200.0, 42.0));
        if ui.add(end).clicked() {
            if self.launch {
                self.installer.launch(&self.folder);
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

/// Installation simulée (tests, et démonstration hors Windows).
#[derive(Default)]
pub struct MockInstaller {
    pub installed: Option<String>,
}

impl Installer for MockInstaller {
    fn default_folder(&self) -> String {
        "C:\\Program Files\\Prism".into()
    }
    fn installed_version(&self) -> Option<String> {
        self.installed.clone()
    }
    fn pick_folder(&self, current: &str) -> Option<String> {
        Some(current.to_string())
    }
    fn start(&mut self, _folder: &str, progress: Shared) {
        if let Ok(mut p) = progress.lock() {
            p.fraction = 0.0;
            p.step = 0;
        }
    }
    fn launch(&self, _folder: &str) {}
}
