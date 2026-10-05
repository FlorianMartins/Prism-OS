//! L'interface : tableau de bord, jeux (et mode console), démarrage, allègement, outils.

use eframe::egui::{self, Align, Color32, CornerRadius, Layout, Margin, RichText, Sense, Stroke, Vec2};
use prism_core::allege::Tier;
use prism_core::library::{Game, Store};
use prism_core::model::human_bytes;

use crate::backend::{AllegeRow, Backend, Live, StartupRow};
use crate::theme as th;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Dashboard,
    Games,
    Startup,
    Allege,
    Services,
    Privacy,
    Appearance,
    Tools,
}

impl Page {
    const ALL: [Page; 8] = [
        Page::Dashboard,
        Page::Games,
        Page::Startup,
        Page::Allege,
        Page::Services,
        Page::Privacy,
        Page::Appearance,
        Page::Tools,
    ];

    fn label(self) -> &'static str {
        match self {
            Page::Dashboard => "Tableau de bord",
            Page::Games => "Jeux",
            Page::Startup => "Démarrage",
            Page::Allege => "Allègement",
            Page::Services => "Services",
            Page::Privacy => "Vie privée",
            Page::Appearance => "Apparence",
            Page::Tools => "Outils cyber",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Page::Dashboard => "📊",
            Page::Games => "🎮",
            Page::Startup => "🚀",
            Page::Allege => "⚡",
            Page::Services => "⚙",
            Page::Privacy => "🔒",
            Page::Appearance => "🖥",
            Page::Tools => "🛡",
        }
    }
}

/// Action longue lancée en arrière-plan : elle reçoit son propre accès au système.
type BgAction = Box<dyn FnOnce(&mut dyn Backend) -> Result<String, String> + Send>;
type ServicesResult = Result<Vec<crate::backend::ServiceRow>, String>;
/// Résultat d'une action d'arrière-plan, rempli par son fil.
type JobSlot = std::sync::Arc<std::sync::Mutex<Option<Result<String, String>>>>;

pub struct PrismApp {
    backend: Box<dyn Backend>,
    /// Fabrique d'un accès au système pour les actions longues (fil d'arrière-plan).
    /// `None` (tests) : l'action est exécutée tout de suite.
    factory: Option<fn() -> Box<dyn Backend>>,
    /// Action en cours : son libellé et son résultat quand elle a fini.
    job: Option<(String, JobSlot)>,
    pub page: Page,
    live: Live,
    startup: Vec<StartupRow>,
    allege: Vec<AllegeRow>,
    /// Plan automatique des jeux à anti-cheat noyau.
    noyau: prism_core::noyau::Reglages,
    /// Fermeture des WebView en arrière-plan.
    webview: prism_core::webview::Reglages,
    /// Nettoyage automatique de la RAM.
    ram_auto: prism_core::ram_auto::Reglages,
    /// Compression de la mémoire (lue à l'ouverture de la page Allègement).
    compression: Option<Option<bool>>,
    compression_loading: Option<std::sync::Arc<std::sync::Mutex<Option<Option<bool>>>>>,
    /// Ce que « Appliquer le thème à tout Windows » applique : accent, fond, sombre, titres.
    design: (bool, bool, bool, bool),
    /// Saisie d'une appli à exclure.
    webview_new: String,
    /// Page Outils : état lu au premier affichage ; désinstallation qui efface des
    /// données en attente de confirmation.
    tools_scan_asked: bool,
    /// Page Services : liste lue à l'ouverture et après chaque changement.
    services: Option<Result<Vec<crate::backend::ServiceRow>, String>>,
    services_search: String,
    /// Page Jeux : réglages par jeu (lus à l'ouverture, les dossiers des jeux sont parcourus).
    game_cfgs: Option<Vec<crate::backend::GameCfg>>,
    services_running_only: bool,
    services_superflus_only: bool,
    services_loading: Option<std::sync::Arc<std::sync::Mutex<Option<ServicesResult>>>>,
    tools_confirm: Option<Vec<String>>,
    games: Vec<Game>,
    privacy: Vec<prism_core::privacy::Row>,
    privacy_conns: Vec<prism_core::privacy::TelemetryConnection>,
    privacy_refresh: f64,
    /// Thème appliqué à l'interface au premier affichage.
    theme_ready: bool,
    /// Accueil du premier lancement déjà vu.
    welcome_done: bool,
    /// Prism au démarrage de Windows.
    autostart_on: bool,
    toast: Option<(String, bool)>,
    /// Notification affichée et depuis quand (animation d'entrée et de sortie).
    toast_shown: Option<(String, f64)>,
    /// Animations : ouverture de l'appli (logo qui se construit), page affichée et
    /// instant où elle l'a été (fondu à l'arrivée). Rien n'anime en continu : au repos,
    /// l'appli ne redessine qu'une fois par seconde (données en direct).
    opened_at: Option<f64>,
    shown_page: Page,
    page_since: f64,
    /// Mode console : lanceur plein écran, navigable au clavier ou à la manette.
    pub console: bool,
    console_sel: usize,
    /// Saisie d'une nouvelle règle de transparence.
    new_rule: String,
    last_refresh: f64,
}

impl PrismApp {
    pub fn new(mut backend: Box<dyn Backend>) -> PrismApp {
        let live = backend.live();
        let startup = backend.startup().unwrap_or_default();
        let allege = backend.allege();
        let noyau = backend.noyau();
        let webview = backend.webview();
        let ram_auto = backend.ram_auto();
        let games = backend.games();
        let (privacy, privacy_conns) = backend.privacy();
        let welcome_done = backend.welcome_done();
        let autostart_on = backend.autostart();
        PrismApp {
            backend,
            factory: None,
            job: None,
            page: Page::Dashboard,
            live,
            startup,
            allege,
            noyau,
            webview,
            ram_auto,
            design: (true, true, true, true),
            compression: None,
            compression_loading: None,
            webview_new: String::new(),
            tools_scan_asked: false,
            services: None,
            services_search: String::new(),
            game_cfgs: None,
            services_running_only: false,
            services_superflus_only: false,
            services_loading: None,
            tools_confirm: None,
            games,
            privacy,
            privacy_conns,
            privacy_refresh: 0.0,
            theme_ready: false,
            welcome_done,
            autostart_on,
            toast: None,
            toast_shown: None,
            opened_at: None,
            shown_page: Page::Dashboard,
            page_since: 0.0,
            console: false,
            console_sel: 0,
            new_rule: String::new(),
            last_refresh: 0.0,
        }
    }

    /// Les actions longues (allègement, vie privée, services, applis…) tournent dans un
    /// fil à part : l'interface ne fige plus pendant qu'elles s'appliquent.
    pub fn with_factory(mut self, factory: fn() -> Box<dyn Backend>) -> PrismApp {
        self.factory = Some(factory);
        self
    }

    fn bg(&mut self, label: &str, f: impl FnOnce(&mut dyn Backend) -> Result<String, String> + Send + 'static) {
        if self.job.is_some() {
            self.toast = Some(("Une action est déjà en cours, un instant…".into(), false));
            return;
        }
        let f: BgAction = Box::new(f);
        let Some(factory) = self.factory else {
            let r = f(self.backend.as_mut());
            self.result(r);
            return;
        };
        let slot = std::sync::Arc::new(std::sync::Mutex::new(None));
        let out = slot.clone();
        std::thread::spawn(move || {
            let mut b = factory();
            let r = f(b.as_mut());
            if let Ok(mut o) = out.lock() {
                *o = Some(r);
            }
        });
        self.job = Some((label.to_string(), slot));
    }

    /// Résultat d'une action d'arrière-plan arrivé : affiché, données relues.
    fn poll_job(&mut self, ctx: &egui::Context) {
        let done = self
            .job
            .as_ref()
            .and_then(|(_, slot)| slot.lock().ok().and_then(|mut o| o.take()));
        match done {
            Some(r) => {
                self.job = None;
                self.result(r);
            }
            None if self.job.is_some() => ctx.request_repaint_after(std::time::Duration::from_millis(200)),
            None => {}
        }
    }

    fn result(&mut self, r: Result<String, String>) {
        self.toast = Some(match r {
            Ok(m) => (m, true),
            Err(e) => (e, false),
        });
        self.startup = self.backend.startup().unwrap_or_default();
        self.allege = self.backend.allege();
        if self.services.is_some() {
            self.services = None;
        }
        if self.game_cfgs.is_some() {
            self.game_cfgs = None;
        }
        (self.privacy, self.privacy_conns) = self.backend.privacy();
        self.live = self.backend.live();
    }

    /// Une frame, dans le `Ui` racine. Séparé de `eframe::App` pour les tests.
    pub fn show(&mut self, root: &mut egui::Ui) {
        let ctx = root.ctx().clone();
        if !self.theme_ready {
            th::set_theme(&ctx, &self.backend.bar_config().theme);
            self.theme_ready = true;
            // Au premier plan dès l'ouverture (lancée par l'installateur, elle pouvait
            // rester derrière les autres fenêtres).
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        self.poll_job(&ctx);
        let now = ctx.input(|i| i.time);
        // Diagnostic (PRISM_UI_DEBUG=1) : images par seconde et ce qui les a demandées,
        // dans %LOCALAPPDATA%\Prism\ui-debug.log.
        if std::env::var_os("PRISM_UI_DEBUG").is_some() {
            debug_frames(&ctx, now);
        }
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        // Redessiner a un coût (mesuré en VM sans carte graphique : une image par seconde
        // = 97 % d'un cœur). Seules les pages en direct se redessinent d'elles-mêmes :
        // le tableau de bord toutes les 2 s (5 s fenêtre en arrière-plan), la vie privée
        // toutes les 3 s ; les autres seulement sur une action.
        let live_every = if focused { 2.0 } else { 5.0 };
        if now - self.last_refresh >= live_every {
            self.live = self.backend.live();
            self.last_refresh = now;
        }
        // Connexions de la télémétrie : relues toutes les 3 s sur la page Vie privée.
        if self.page == Page::Privacy && now - self.privacy_refresh > 3.0 {
            (self.privacy, self.privacy_conns) = self.backend.privacy();
            self.privacy_refresh = now;
        }
        match self.page {
            Page::Dashboard => ctx.request_repaint_after(std::time::Duration::from_secs_f64(live_every)),
            Page::Privacy => ctx.request_repaint_after(std::time::Duration::from_secs(3)),
            _ => {}
        }

        if self.console {
            self.console_ui(root);
            return;
        }

        egui::Panel::left("nav")
            .exact_size(212.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(th::panel())
                    .inner_margin(Margin::symmetric(14, 18)),
            )
            .show(root, |ui| self.nav(ui));

        egui::Panel::top("header")
            .exact_size(58.0)
            .frame(
                egui::Frame::new()
                    .fill(th::bg())
                    .inner_margin(Margin::symmetric(24, 12)),
            )
            .show(root, |ui| self.header(ui));

        // Changement de page : fondu et léger glissement (0,18 s).
        if self.shown_page != self.page {
            self.shown_page = self.page;
            self.page_since = now;
        }
        let k = ((now - self.page_since) / 0.18).clamp(0.0, 1.0) as f32;
        if k < 1.0 {
            ctx.request_repaint();
        }
        let ease = 1.0 - (1.0 - k).powi(3);
        // Cartes en cascade : instant du changement de page, compteur remis à zéro.
        ctx.data_mut(|d| {
            d.insert_temp(egui::Id::new("prism-page-since"), self.page_since);
            d.insert_temp(egui::Id::new("prism-card-idx"), 0u32);
        });
        egui::CentralPanel::no_frame()
            .frame(egui::Frame::new().fill(th::bg()).inner_margin(Margin::symmetric(24, 8)))
            .show(root, |ui| {
                // Fond technologique (grille fine, halos), statique.
                crate::futur::backdrop(ui.painter(), ui.clip_rect());
                ui.set_opacity(ease);
                ui.add_space((1.0 - ease) * 12.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| match self.page {
                        Page::Dashboard => self.dashboard(ui),
                        Page::Games => self.games_page(ui),
                        Page::Startup => self.startup_page(ui),
                        Page::Allege => self.allege_page(ui),
                        Page::Privacy => self.privacy_page(ui),
                        Page::Services => self.services_page(ui),
                        Page::Appearance => self.appearance_page(ui),
                        Page::Tools => self.tools_page(ui),
                    });
            });
        self.toast_area(&ctx);
    }

    /// Notification flottante : glisse depuis le bas à droite, s'efface après 6 s.
    fn toast_area(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        let Some((msg, ok)) = self.toast.clone() else {
            self.toast_shown = None;
            return;
        };
        let start = match &self.toast_shown {
            Some((m, t)) if *m == msg => *t,
            _ => {
                self.toast_shown = Some((msg.clone(), now));
                now
            }
        };
        let age = now - start;
        if age > 6.4 {
            self.toast = None;
            self.toast_shown = None;
            return;
        }
        let k_in = (age / 0.25).clamp(0.0, 1.0) as f32;
        let k_out = ((6.4 - age) / 0.4).clamp(0.0, 1.0) as f32;
        let a = (1.0 - (1.0 - k_in).powi(3)) * k_out;
        ctx.request_repaint_after(std::time::Duration::from_millis(if k_in < 1.0 || k_out < 1.0 {
            16
        } else {
            300
        }));
        let col = if ok { th::ok() } else { th::bad() };
        egui::Area::new(egui::Id::new("prism-toast"))
            .anchor(egui::Align2::RIGHT_BOTTOM, Vec2::new(-20.0 + (1.0 - a) * 40.0, -20.0))
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                ui.set_opacity(a);
                egui::Frame::new()
                    .fill(th::card_hi())
                    .stroke(Stroke::new(1.0, col.gamma_multiply(0.7)))
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(Margin::symmetric(14, 10))
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 6],
                        blur: 18,
                        spread: 0,
                        color: Color32::from_black_alpha(90),
                    })
                    .show(ui, |ui| {
                        ui.set_max_width(420.0);
                        ui.horizontal(|ui| {
                            let (r, _) = ui.allocate_exact_size(Vec2::new(4.0, 30.0), Sense::hover());
                            ui.painter().rect_filled(r, 2.0, col);
                            ui.label(RichText::new(&msg).color(th::text()));
                        });
                    });
            });
    }

    fn nav(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            // Logo : se construit à l'ouverture (2,5 s), tourne au survol, immobile sinon.
            let now = ui.input(|i| i.time);
            let opened = *self.opened_at.get_or_insert(now);
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(40.0, 46.0), Sense::hover());
            let since = (now - opened) as f32;
            let spin_id = ui.id().with("logo-spin");
            let spin = ui.ctx().animate_bool_with_time(spin_id, resp.hovered(), 0.4);
            let angle_id = ui.id().with("logo-angle");
            let mut angle: f32 = ui.ctx().data(|d| d.get_temp(angle_id)).unwrap_or(0.6);
            if since < 2.5 || spin > 0.0 {
                angle += 0.03 * (1.0 + spin * 1.5);
                ui.ctx().data_mut(|d| d.insert_temp(angle_id, angle));
                ui.ctx().request_repaint();
            }
            crate::logo::draw_prism(ui.painter(), rect.center(), 15.0, angle, since * 2.0, false, now as f32);
            ui.vertical(|ui| {
                ui.label(RichText::new("PRISM").size(20.0).strong().color(th::text()));
                ui.label(RichText::new("gaming · cybersécurité").small().color(th::muted()));
            });
        });
        ui.add_space(26.0);
        for p in Page::ALL {
            let selected = self.page == p;
            // Survol : le fond s'éclaire en 0,12 s (état du survol de l'image précédente).
            let hid = ui.id().with(("nav-hover", p.label()));
            let was_hovered: bool = ui.ctx().data(|d| d.get_temp(hid)).unwrap_or(false);
            let h = ui
                .ctx()
                .animate_bool_with_time(hid.with("a"), was_hovered && !selected, 0.12);
            let (fill, fg) = if selected {
                (th::accent_dim(), th::accent())
            } else {
                (th::card().gamma_multiply(h), th::text())
            };
            let resp = egui::Frame::new()
                .fill(fill)
                .corner_radius(CornerRadius::same(8))
                .inner_margin(Margin::symmetric(12, 9))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(p.icon()).size(16.0).color(fg));
                        ui.label(RichText::new(p.label()).color(fg));
                    });
                })
                .response
                .interact(Sense::click());
            if selected {
                // Repère lumineux à gauche de l'entrée active.
                let r = resp.rect;
                let bar = egui::Rect::from_min_max(
                    egui::pos2(r.left() - 6.0, r.top() + 8.0),
                    egui::pos2(r.left() - 3.0, r.bottom() - 8.0),
                );
                ui.painter().rect_filled(bar, 2.0, th::accent());
                crate::futur::glow(ui.painter(), bar.center(), 18.0, th::accent(), 40);
            }
            ui.ctx().data_mut(|d| d.insert_temp(hid, resp.hovered()));
            if resp.clicked() {
                self.page = p;
            }
            ui.add_space(2.0);
        }
        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            ui.label(
                RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                    .small()
                    .color(th::muted()),
            );
        });
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        let now = ui.input(|i| i.time);
        let since = self.page_since;
        let job = self.job.as_ref().map(|(l, _)| l.clone());
        ui.horizontal_centered(|ui| {
            let title = ui.heading(self.page.label());
            // Trait lumineux qui se déploie sous le titre à chaque page.
            let k = ((now - since) / 0.45).clamp(0.0, 1.0) as f32;
            if k < 1.0 {
                ui.ctx().request_repaint();
            }
            crate::futur::underline(
                ui.painter(),
                title.rect.left_bottom() + Vec2::new(0.0, 4.0),
                title.rect.width() + 24.0,
                1.0 - (1.0 - k).powi(3),
            );
            // Action en cours (fil d'arrière-plan).
            if let Some(label) = &job {
                ui.add_space(12.0);
                ui.spinner();
                ui.label(RichText::new(format!("{label}…")).small().color(th::accent()));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if !self.backend.elevated() && ui.button("Relancer en administrateur").clicked() {
                    let r = self.backend.relaunch_elevated();
                    self.result(r);
                }
                if self.live.watch_alive {
                    pill(ui, "✔ Prism actif", th::ok());
                } else {
                    if ui.button("Démarrer Prism").clicked() {
                        let r = self.backend.start_watch();
                        self.result(r);
                    }
                    pill(ui, "✖ Prism arrêté", th::bad());
                }
                if let Some(e) = &self.live.etat {
                    if !e.game.is_empty() {
                        pill(ui, &format!("🎮 Mode Jeu : {}", e.game.join(", ")), th::accent());
                    }
                }
            });
        });
    }

    // --- Tableau de bord ------------------------------------------------------

    fn dashboard(&mut self, ui: &mut egui::Ui) {
        if !self.welcome_done {
            self.welcome(ui);
            ui.add_space(12.0);
        }
        // Système en direct : jauges circulaires.
        section(ui, "Système");
        let m0 = self.live.mem;
        let used_pct = (m0.total > 0)
            .then(|| (m0.total.saturating_sub(m0.free + m0.standby_total) as f64 * 100.0 / m0.total as f64) as f32);
        let (cpu, gpu) = (self.live.cpu, self.live.gpu);
        let cores = self.live.cores.clone();
        let used_txt = format!(
            "{} utilisés sur {}",
            human_bytes(m0.total.saturating_sub(m0.free + m0.standby_total)),
            human_bytes(m0.total)
        );
        row(ui, 3, |i, col| {
            card(col, false, |ui| match i {
                0 => crate::futur::ring(ui, cpu, "Processeur", &cores),
                1 => crate::futur::ring(ui, used_pct, "Mémoire", &used_txt),
                _ => crate::futur::ring(
                    ui,
                    gpu,
                    "Carte graphique",
                    if gpu.is_some() {
                        "moteur le plus chargé"
                    } else {
                        "compteur indisponible"
                    },
                ),
            });
        });
        ui.add_space(14.0);
        section(ui, "Profil");
        let profiles = self.backend.profiles();
        let mut chosen = None;
        let current = self.live.profile.clone();
        row(ui, profiles.len().max(1), |i, col| {
            let p = &profiles[i];
            let active = p.name == current;
            let resp = card(col, active, |ui| {
                ui.label(RichText::new(&p.label).size(17.0).strong().color(if active {
                    th::accent()
                } else {
                    th::text()
                }));
                ui.add_space(2.0);
                ui.label(RichText::new(&p.description).small().color(th::muted()));
            });
            if resp.interact(Sense::click()).clicked() && !active {
                chosen = Some(p.name.clone());
            }
        });
        if let Some(name) = chosen {
            let r = self.backend.set_profile(&name);
            self.result(r);
        }

        ui.add_space(14.0);
        section(ui, "En ce moment");
        let m = self.live.mem;
        let eased = self.live.etat.as_ref().map(|e| e.eased.clone()).unwrap_or_default();
        let game = self.live.etat.as_ref().map(|e| e.game.clone()).unwrap_or_default();
        let mut clean = false;
        let mut rapport = false;
        let mut ram_auto = self.ram_auto.clone();
        let cores = self.live.cores.clone();
        row(ui, 3, |i, col| match i {
            0 => {
                card(col, false, |ui| {
                    stat_title(ui, "Mémoire");
                    let used = m.total.saturating_sub(m.free);
                    ui.label(
                        RichText::new(format!("{} libres", human_bytes(m.free)))
                            .size(22.0)
                            .strong(),
                    );
                    bar(ui, if m.total > 0 { used as f32 / m.total as f32 } else { 0.0 });
                    ui.label(
                        RichText::new(format!(
                            "sur {} · cache {} dont {} libérable",
                            human_bytes(m.total),
                            human_bytes(m.standby_total),
                            human_bytes(m.standby_low)
                        ))
                        .small()
                        .color(th::muted()),
                    );
                    if ui.button("Libérer la RAM des applis inactives").clicked() {
                        clean = true;
                    }
                    if ui
                        .button("Rapport mémoire")
                        .on_hover_text("Où part la RAM de ce PC, avec des conseils. Fichier texte sans données personnelles, à envoyer tel quel.")
                        .clicked()
                    {
                        rapport = true;
                    }
                    // Nettoyage automatique : la même action, lancée par le moteur.
                    ui.add_space(6.0);
                    let ra = &mut ram_auto;
                    ui.checkbox(&mut ra.actif, "Nettoyage automatique")
                        .on_hover_text("Le moteur libère la RAM des applis inactives tout seul, comme le bouton ci-dessus (jamais pendant une partie).");
                    ui.add_enabled_ui(ra.actif, |ui| {
                        ui.add(
                            egui::Slider::new(&mut ra.toutes_les_minutes, 5..=120)
                                .prefix("toutes les ")
                                .suffix(" min"),
                        );
                        ui.add(
                            egui::Slider::new(&mut ra.seuil_pourcent, 50..=95)
                                .prefix("et dès ")
                                .suffix(" % utilisés"),
                        );
                    });
                });
            }
            1 => {
                card(col, false, |ui| {
                    stat_title(ui, "Mode Quotidien");
                    ui.label(
                        RichText::new(format!("{} appli(s) allégée(s)", eased.len()))
                            .size(22.0)
                            .strong(),
                    );
                    ui.label(RichText::new("Inactives : cœurs économes, basse consommation, RAM rendue. Tout revient dès que vous y retournez.").small().color(th::muted()));
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        for n in eased.iter().take(8) {
                            chip(ui, n);
                        }
                    });
                });
            }
            _ => {
                card(col, false, |ui| {
                    stat_title(ui, "Mode Jeu");
                    if game.is_empty() {
                        ui.label(RichText::new("En attente").size(22.0).strong().color(th::muted()));
                        ui.label(
                            RichText::new("S'active tout seul au lancement d'un jeu.")
                                .small()
                                .color(th::muted()),
                        );
                    } else {
                        ui.label(RichText::new(game.join(", ")).size(22.0).strong().color(th::accent()));
                        ui.label(
                            RichText::new("Arrière-plan en retrait, RAM rendue, indexation en pause.")
                                .small()
                                .color(th::muted()),
                        );
                    }
                    ui.add_space(4.0);
                    ui.label(RichText::new(&cores).small().color(th::muted()));
                });
            }
        });
        if clean {
            self.bg("Libération de la RAM", |b| b.ram_clean());
        }
        if ram_auto != self.ram_auto {
            self.ram_auto = ram_auto;
            let r = self.backend.set_ram_auto(&self.ram_auto);
            if let Err(e) = r {
                self.toast = Some((e, false));
            }
        }
        if rapport {
            self.bg("Rapport mémoire", |b| b.rapport());
        }

        ui.add_space(14.0);
        let top = self.live.top.clone();
        let recent = self.live.etat.as_ref().map(|e| e.recent.clone()).unwrap_or_default();
        row(ui, 2, |i, col| {
            if i == 0 {
                section(col, "Processus les plus actifs");
                card(col, false, |ui| {
                    egui::Grid::new("top")
                        .num_columns(4)
                        .spacing([18.0, 6.0])
                        .striped(true)
                        .show(ui, |ui| {
                            for h in ["Processus", "CPU", "RAM", "Classe"] {
                                ui.label(RichText::new(h).small().color(th::muted()));
                            }
                            ui.end_row();
                            for p in top.iter().take(9) {
                                ui.label(&p.name);
                                ui.label(format!("{:.1} %", p.cpu_percent));
                                ui.label(human_bytes(p.ram));
                                ui.label(RichText::new(&p.class).color(class_color(&p.class)));
                                ui.end_row();
                            }
                        });
                });
            } else {
                section(col, "Activité récente");
                card(col, false, |ui| {
                    if recent.is_empty() {
                        ui.label(RichText::new("Rien pour l'instant.").color(th::muted()));
                    }
                    for line in recent.iter().rev().take(10) {
                        ui.label(RichText::new(line).small());
                    }
                });
            }
        });
        ui.add_space(12.0);
        card(ui, false, |ui| {
            let mut on = self.autostart_on;
            if ui
                .checkbox(&mut on, RichText::new("Prism au démarrage de Windows").strong())
                .on_hover_text("Le moteur (qui allège la RAM et le processeur) et la Prism Bar se lancent à chaque ouverture de session")
                .changed()
            {
                let r = self.backend.set_autostart(on);
                self.autostart_on = self.backend.autostart();
                self.result(r);
            }
            ui.label(
                RichText::new("Sans lui, Prism ne fait rien après un redémarrage : c'est le moteur qui libère la mémoire des applis inactives.")
                    .small()
                    .color(th::muted()),
            );
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!(
                    "Prism {} · mises à jour : prism maj installer",
                    env!("CARGO_PKG_VERSION")
                ))
                .small()
                .color(th::muted()),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .button("Désinstaller Prism")
                    .on_hover_text(
                        "Remet tout comme avant Prism, puis retire le programme (Windows demande confirmation)",
                    )
                    .clicked()
                {
                    let r = self.backend.uninstall();
                    self.result(r);
                }
            });
        });
    }

    // --- Jeux -------------------------------------------------------------------

    fn games_page(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!(
                    "{} jeu(x) installé(s) détecté(s) : Steam, Epic, GOG, Battle.net.",
                    self.games.len()
                ))
                .color(th::muted()),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("🕹  Mode console").clicked() {
                    self.console = true;
                    self.console_sel = 0;
                }
            });
        });
        ui.add_space(8.0);
        let mut launch = None;
        let cols = ((ui.available_width() + 14.0) / 214.0).floor().max(1.0) as usize;
        egui::Grid::new("games").spacing([14.0, 14.0]).show(ui, |ui| {
            for (i, g) in self.games.iter().enumerate() {
                if game_tile(ui, g, Vec2::new(200.0, 118.0), false).clicked() {
                    launch = Some(g.clone());
                }
                if (i + 1) % cols == 0 {
                    ui.end_row();
                }
            }
        });
        if let Some(g) = launch {
            let r = self.backend.launch(&g);
            self.result(r);
        }
        ui.add_space(16.0);
        self.games_settings(ui);
    }

    /// Dernière partie et réglages Windows par jeu (carte graphique, plein écran).
    fn games_settings(&mut self, ui: &mut egui::Ui) {
        use prism_core::jeux::Reglage;
        if let Some(p) = self.backend.last_session() {
            section(ui, "Dernière partie");
            card(ui, false, |ui| {
                ui.label(
                    RichText::new(format!(
                        "{} · {} · {} h {:02} min",
                        p.jeux.join(", "),
                        p.debut,
                        p.duree_secs / 3600,
                        p.duree_secs % 3600 / 60
                    ))
                    .strong(),
                );
                let go = |b: u64| format!("{:.1} Go", b as f64 / (1u64 << 30) as f64);
                ui.label(
                    RichText::new(format!(
                        "{} réglage(s) du Mode Jeu appliqués au lancement, remis à la fin · mémoire disponible : {} au début, {} au plus bas{}",
                        p.actions,
                        go(p.dispo_debut),
                        go(p.dispo_min),
                        p.anticheat
                            .as_ref()
                            .map(|a| format!(" · {a} : services et outils préparés"))
                            .unwrap_or_default()
                    ))
                    .small()
                    .color(th::muted()),
                );
            });
            ui.add_space(12.0);
        }
        section(ui, "Réglages par jeu");
        ui.label(
            RichText::new("Réglages de Windows pour l'exécutable du jeu, comme dans Paramètres > Graphiques et Propriétés > Compatibilité : rien n'est écrit dans le jeu, compatible avec les anti-cheats. Pris en compte au prochain lancement ; décocher remet la valeur d'origine.")
                .small()
                .color(th::muted()),
        );
        if self.game_cfgs.is_none() {
            self.game_cfgs = Some(self.backend.game_cfgs());
        }
        let cfgs = self.game_cfgs.clone().unwrap_or_default();
        let mut set: Option<(String, Reglage, bool)> = None;
        card(ui, false, |ui| {
            egui::Grid::new("jeux-reglages")
                .num_columns(3)
                .spacing([18.0, 6.0])
                .striped(true)
                .show(ui, |ui| {
                    ui.label(RichText::new("Jeu").small().color(th::muted()));
                    ui.label(
                        RichText::new("Carte graphique haute performance")
                            .small()
                            .color(th::muted()),
                    );
                    ui.label(RichText::new("Plein écran exclusif").small().color(th::muted()));
                    ui.end_row();
                    for g in &cfgs {
                        let name = ui.label(&g.name);
                        if g.exes.is_empty() {
                            name.on_hover_text("Aucun exécutable trouvé dans son dossier");
                            ui.label(RichText::new("–").color(th::muted()));
                            ui.label(RichText::new("–").color(th::muted()));
                        } else {
                            name.on_hover_text(g.exes.join("\n"));
                            for (r, v) in [(Reglage::Gpu, g.gpu), (Reglage::PleinEcran, g.plein_ecran)] {
                                let mut on = v.unwrap_or(false);
                                if ui
                                    .add(egui::Checkbox::without_text(&mut on))
                                    .on_hover_text(r.label())
                                    .changed()
                                {
                                    set = Some((g.name.clone(), r, on));
                                }
                            }
                        }
                        ui.end_row();
                    }
                });
        });
        if let Some((name, r, on)) = set {
            self.bg("Réglage du jeu", move |b| b.game_set(&name, r, on));
        }
    }

    fn console_ui(&mut self, root: &mut egui::Ui) {
        let ctx = root.ctx().clone();
        let n = self.games.len();
        let cols = 4usize;
        ctx.input(|i| {
            if i.key_pressed(egui::Key::ArrowRight) && self.console_sel + 1 < n {
                self.console_sel += 1;
            }
            if i.key_pressed(egui::Key::ArrowLeft) && self.console_sel > 0 {
                self.console_sel -= 1;
            }
            if i.key_pressed(egui::Key::ArrowDown) && self.console_sel + cols < n {
                self.console_sel += cols;
            }
            if i.key_pressed(egui::Key::ArrowUp) && self.console_sel >= cols {
                self.console_sel -= cols;
            }
        });
        let enter = ctx.input(|i| i.key_pressed(egui::Key::Enter));
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.console = false;
        }
        let mut launch = None;
        egui::CentralPanel::no_frame()
            .frame(
                egui::Frame::new()
                    .fill(th::bg())
                    .inner_margin(Margin::symmetric(56, 40)),
            )
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("♦ PRISM").size(28.0).strong().color(th::accent()));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new("Flèches : choisir · Entrée : jouer · Échap : quitter").color(th::muted()),
                        );
                    });
                });
                ui.add_space(30.0);
                egui::Grid::new("console").spacing([22.0, 22.0]).show(ui, |ui| {
                    for (i, g) in self.games.iter().enumerate() {
                        let sel = i == self.console_sel;
                        if game_tile(ui, g, Vec2::new(280.0, 168.0), sel).clicked() || (sel && enter) {
                            launch = Some(g.clone());
                        }
                        if (i + 1) % cols == 0 {
                            ui.end_row();
                        }
                    }
                });
            });
        if let Some(g) = launch {
            let r = self.backend.launch(&g);
            self.result(r);
        }
    }

    // --- Démarrage --------------------------------------------------------------

    fn startup_page(&mut self, ui: &mut egui::Ui) {
        let mut action: Option<Result<String, String>> = None;
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(
                    "Désactiver une appli au démarrage ne la supprime pas : elle se lance quand vous l'ouvrez.",
                )
                .color(th::muted()),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Tout restaurer").clicked() {
                    self.bg("Démarrage : restauration", |b| b.startup_restore());
                }
                if ui
                    .add(
                        egui::Button::new(RichText::new("Appliquer les conseils").color(th::on_accent()))
                            .fill(th::accent()),
                    )
                    .clicked()
                {
                    self.bg("Démarrage : réglages recommandés", |b| b.startup_recommended());
                }
            });
        });
        ui.add_space(8.0);
        let rows = self.startup.clone();
        for r in rows {
            card(ui, false, |ui| {
                ui.horizontal(|ui| {
                    let mut on = r.enabled;
                    let toggle = ui.add_enabled(!r.protected, toggle_switch(&mut on));
                    if toggle.changed() {
                        action = Some(self.backend.startup_toggle(r.source, &r.name, on));
                    }
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            let (shown, full) = display_name(&r);
                            let l = ui.label(RichText::new(shown).strong());
                            if let Some(full) = full {
                                l.on_hover_text(full);
                            }
                            badge(ui, &r.advice);
                            ui.label(RichText::new(r.source.label()).small().color(th::muted()));
                        });
                        ui.label(RichText::new(&r.why).small().color(th::muted()));
                    });
                });
            });
            ui.add_space(4.0);
        }
        if let Some(r) = action {
            self.result(r);
        }
    }

    // --- Allègement -------------------------------------------------------------

    fn privacy_page(&mut self, ui: &mut egui::Ui) {
        use prism_core::privacy::{score, Category, Kind, Level, State};
        let mut toggle: Option<(String, bool)> = None;
        let action: Option<Result<String, String>> = None;
        let (on, total) = score(&self.privacy);
        card(ui, true, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(format!("{on} / {total}"))
                            .size(30.0)
                            .strong()
                            .color(th::accent()),
                    );
                    ui.label(RichText::new("protections en place").color(th::muted()));
                });
                ui.add_space(16.0);
                ui.vertical(|ui| {
                    ui.add(
                        egui::ProgressBar::new(if total > 0 { on as f32 / total as f32 } else { 0.0 })
                            .desired_width(260.0)
                            .fill(th::accent()),
                    );
                    ui.label(
                        RichText::new(
                            "Stratégies officielles de Windows et règles du Pare-feu Windows, tout réversible.",
                        )
                        .small()
                        .color(th::muted()),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Tout restaurer").clicked() {
                        self.bg("Vie privée : restauration", |b| b.privacy_restore());
                    }
                    if ui
                        .button("Strict")
                        .on_hover_text(
                            "Recommandé + réglages où l'on renonce à quelque chose (détaillé ligne par ligne)",
                        )
                        .clicked()
                    {
                        self.bg("Vie privée : niveau strict", |b| b.privacy_apply(Level::Strict));
                    }
                    if ui
                        .add(
                            egui::Button::new(RichText::new("Recommandé").color(th::on_accent()).strong())
                                .fill(th::accent()),
                        )
                        .on_hover_text("Aucune fonction utile perdue")
                        .clicked()
                    {
                        self.bg("Vie privée : niveau recommandé", |b| {
                            b.privacy_apply(Level::Recommande)
                        });
                    }
                });
            });
        });
        ui.add_space(12.0);
        section(ui, "Télémétrie en ce moment");
        card(ui, false, |ui| {
            if self.privacy_conns.is_empty() {
                ui.label(
                    RichText::new("✔  Aucun composant de télémétrie surveillé ne communique en ce moment.")
                        .color(th::ok()),
                );
            } else {
                ui.label(
                    RichText::new(format!(
                        "⚠  {} connexion(s) ouverte(s) par la télémétrie",
                        self.privacy_conns.len()
                    ))
                    .color(th::warn()),
                );
                egui::Grid::new("tele")
                    .num_columns(3)
                    .spacing([16.0, 4.0])
                    .show(ui, |ui| {
                        for k in &self.privacy_conns {
                            ui.label(&k.component);
                            ui.label(RichText::new(&k.remote).monospace().color(th::muted()));
                            ui.label(RichText::new(&k.state).small().color(th::muted()));
                            ui.end_row();
                        }
                    });
            }
        });
        for cat in Category::ALL {
            let rows: Vec<_> = self.privacy.iter().filter(|r| r.category == cat).cloned().collect();
            if rows.is_empty() {
                continue;
            }
            let (c_on, c_total) = score(&rows);
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                section(ui, cat.label());
                ui.label(RichText::new(format!("{c_on}/{c_total}")).small().color(th::muted()));
            });
            card(ui, false, |ui| {
                egui::Grid::new(format!("vp-{cat:?}"))
                    .num_columns(3)
                    .spacing([16.0, 6.0])
                    .striped(true)
                    .show(ui, |ui| {
                        for r in &rows {
                            match (&r.key, &r.state) {
                                // Réglage ou règle de Prism : une case par ligne.
                                (Some(key), State::On | State::Off) => {
                                    let mut on = r.state == State::On;
                                    if ui.add(egui::Checkbox::without_text(&mut on)).changed() {
                                        toggle = Some((key.clone(), on));
                                    }
                                }
                                _ => {
                                    let (mark, color) = match &r.state {
                                        State::On => ("✔", th::ok()),
                                        State::Off => ("·", th::muted()),
                                        State::Absent => ("–", th::muted()),
                                        State::Unknown(_) => ("⚠", th::warn()),
                                    };
                                    ui.label(RichText::new(mark).color(color));
                                }
                            }
                            let mut tip = r.why.clone();
                            if let Some(l) = &r.lose {
                                tip.push_str(&format!("\n\nOn renonce à : {l}"));
                            }
                            if let State::Unknown(e) = &r.state {
                                tip.push_str(&format!("\n\n{e}"));
                            }
                            let resp = ui.vertical(|ui| {
                                // Colonne du libellé assez large pour une ligne (sinon la
                                // grille la réduit à un mot par ligne).
                                ui.set_min_width(620.0);
                                ui.label(&r.label);
                                if let Some(l) = &r.lose {
                                    ui.label(RichText::new(format!("On renonce à : {l}")).small().color(th::muted()));
                                }
                            });
                            if !tip.is_empty() {
                                resp.response.on_hover_text(tip);
                            }
                            let tag = match (r.kind, r.level) {
                                (Kind::Check, _) => format!("via {}", r.source),
                                (Kind::Firewall, Some(l)) => format!("{} · pare-feu", l.label()),
                                (_, Some(l)) if r.reboot => format!("{} · à la reconnexion", l.label()),
                                (_, Some(l)) => l.label().to_string(),
                                _ => String::new(),
                            };
                            ui.label(RichText::new(tag).small().color(th::muted()));
                            ui.end_row();
                        }
                    });
            });
        }
        if let Some((key, on)) = toggle {
            self.bg("Vie privée", move |b| b.privacy_toggle(&key, on));
        }
        if let Some(r) = action {
            self.result(r);
        }
    }

    /// Premier lancement : ce que fait Prism, et de quoi le voir en un clic.
    fn welcome(&mut self, ui: &mut egui::Ui) {
        let mut action: Option<Result<String, String>> = None;
        card(ui, true, |ui| {
            ui.label(
                RichText::new("Bienvenue dans Prism")
                    .size(22.0)
                    .strong()
                    .color(th::accent()),
            );
            ui.label(
                RichText::new(
                    "Prism ne change rien tant que vous ne le demandez pas. Pour voir la différence tout de suite : \
                     la Prism Bar remplace votre barre des tâches, et les fenêtres s'animent (lampe de génie, \
                     gélatine au déplacement, glisse en agrandissant). Tout se règle dans Apparence et s'annule d'un clic.",
                )
                .color(th::text()),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let go = egui::Button::new(
                    RichText::new("Lancer la Prism Bar et activer les effets")
                        .color(th::on_accent())
                        .strong(),
                )
                .fill(th::accent());
                if ui.add(go).clicked() {
                    let mut cfg = self.backend.bar_config();
                    cfg.fx.enabled = true;
                    let saved = self.backend.set_bar_config(&cfg);
                    // Comme le fait la case « Activer les effets » : l'animation de
                    // réduction de Windows est coupée pour ne pas se superposer.
                    if let Some(row) = self.backend.appearance().into_iter().find(|r| r.id == "anim_minmax") {
                        if let Some(i) = row.options.iter().position(|o| o == "Instantanée") {
                            let _ = self.backend.appearance_set("anim_minmax", i);
                        }
                    }
                    let started = if self.backend.bar_running() {
                        Ok("Prism Bar déjà lancée".into())
                    } else {
                        self.backend.bar_start()
                    };
                    // Et au démarrage de Windows : c'est le moteur qui allège la RAM.
                    let boot = self.backend.set_autostart(true);
                    action = Some(
                        saved
                            .and(started)
                            .and(boot)
                            .map(|_| "Prism Bar lancée, effets activés, Prism au démarrage de Windows".into()),
                    );
                    self.backend.set_welcome_done();
                    self.welcome_done = true;
                    self.page = Page::Appearance;
                }
                if ui.button("Plus tard").clicked() {
                    self.backend.set_welcome_done();
                    self.welcome_done = true;
                }
            });
        });
        if let Some(r) = action {
            self.result(r);
        }
    }

    fn allege_page(&mut self, ui: &mut egui::Ui) {
        let mut action: Option<Result<String, String>> = None;
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(
                    "72 services dont les anti-cheats et les mises à jour ont besoin ne sont jamais touchés. Tout est réversible (« Tout restaurer »).",
                )
                .color(th::muted()),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Tout restaurer").clicked() {
                    self.bg("Allègement : restauration", |b| b.allege_restore());
                }
            });
        });
        ui.add_space(10.0);
        self.compression_section(ui);
        ui.add_space(10.0);
        self.noyau_section(ui, &mut action);
        ui.add_space(12.0);
        self.webview_section(ui, &mut action);
        let mut toggle: Option<(String, bool)> = None;
        for (tier, title, note) in [
            (
                Tier::Sur,
                "Sûr",
                "Vie privée et services inutiles : aucune fonction utile perdue.",
            ),
            (
                Tier::Avance,
                "Avancé",
                "Gain réel, contrepartie expliquée pour chaque ligne.",
            ),
            (
                Tier::Jeu,
                "Réglages jeu",
                "Mode Jeu Windows, GPU, jeux en fenêtre, souris.",
            ),
            (
                Tier::Extreme,
                "Extrême",
                "Services d'arrière-plan à la demande, Widgets, Copilot, applis préinstallées retirées. Remis tout seuls pendant un jeu à anti-cheat noyau.",
            ),
        ] {
            ui.add_space(12.0);
            let rows: Vec<AllegeRow> = self.allege.iter().filter(|r| r.tier == tier).cloned().collect();
            let done = rows.iter().filter(|r| r.done).count();
            ui.horizontal(|ui| {
                section(ui, title);
                ui.label(
                    RichText::new(format!("{done}/{} appliqué(s) · {note}", rows.len()))
                        .small()
                        .color(th::muted()),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button(format!("Appliquer « {title} »")).clicked() {
                        self.bg("Allègement", move |b| b.allege_apply(tier));
                    }
                });
            });
            card(ui, false, |ui| {
                egui::Grid::new(format!("allege-{title}"))
                    .num_columns(3)
                    .spacing([16.0, 6.0])
                    .striped(true)
                    .show(ui, |ui| {
                        for r in &rows {
                            // Case par élément : cochée = appliqué. Décocher remet la valeur
                            // d'origine, seulement si c'est Prism qui l'a changée.
                            let mut on = r.done;
                            let can = !r.done || r.by_prism;
                            let resp = ui
                                .add_enabled(can, egui::Checkbox::without_text(&mut on))
                                .on_disabled_hover_text("Déjà ainsi avant Prism : rien à remettre");
                            if resp.changed() {
                                toggle = Some((r.key.clone(), on));
                            }
                            ui.label(&r.label).on_hover_text(&r.why);
                            ui.label(
                                RichText::new(format!("{}  ›  {}", r.current, r.target))
                                    .small()
                                    .color(th::muted()),
                            );
                            ui.end_row();
                        }
                    });
            });
        }
        if let Some((key, on)) = toggle {
            self.bg("Allègement", move |b| b.allege_toggle(&key, on));
        }
        if let Some(r) = action {
            self.result(r);
        }
    }

    /// Compression de la mémoire de Windows : désactivée, la RAM rendue par Prism quitte
    /// vraiment la mémoire au lieu d'y rester compressée.
    fn compression_section(&mut self, ui: &mut egui::Ui) {
        // Lue en arrière-plan (PowerShell, environ une seconde).
        if self.compression.is_none() && self.compression_loading.is_none() {
            match self.factory {
                Some(factory) => {
                    let slot = std::sync::Arc::new(std::sync::Mutex::new(None));
                    let out = slot.clone();
                    std::thread::spawn(move || {
                        let v = factory().compression();
                        if let Ok(mut o) = out.lock() {
                            *o = Some(v);
                        }
                    });
                    self.compression_loading = Some(slot);
                }
                None => self.compression = Some(self.backend.compression()),
            }
        }
        if let Some(slot) = &self.compression_loading {
            match slot.lock().ok().and_then(|mut o| o.take()) {
                Some(v) => {
                    self.compression = Some(v);
                    self.compression_loading = None;
                }
                None => ui.ctx().request_repaint_after(std::time::Duration::from_millis(300)),
            }
        }
        let Some(state) = self.compression.flatten() else {
            return;
        };
        let total = self.live.mem.total;
        section(ui, "Compression de la mémoire");
        let mut toggle = None;
        card(ui, false, |ui| {
            ui.horizontal(|ui| {
                let mut on = !state;
                if ui.add(toggle_switch(&mut on)).changed() {
                    toggle = Some(!on);
                }
                ui.label(RichText::new("Désactiver la compression de la mémoire").strong());
            });
            ui.label(
                RichText::new(format!(
                    "Quand Prism rend la RAM d'une appli inactive, Windows la compresse et la garde en mémoire (processus « Memory Compression »), elle compte toujours comme utilisée. Désactivée, elle part vraiment. Conseillé à partir de 16 Go de RAM (ce PC : {}). Pris en compte au prochain redémarrage ; « Tout restaurer » remet le réglage d'origine.",
                    human_bytes(total)
                ))
                .small()
                .color(th::muted()),
            );
        });
        if let Some(on) = toggle {
            self.compression = None;
            self.bg("Compression de la mémoire", move |b| b.set_compression(on));
        }
    }

    /// WebView des applis restées sans fenêtre (Teams, Outlook, Widgets dans la zone de
    /// notification…) : fermées après un délai.
    fn webview_section(&mut self, ui: &mut egui::Ui, action: &mut Option<Result<String, String>>) {
        section(ui, "WebView en arrière-plan");
        ui.label(
            RichText::new(
                "Une appli réduite dans la zone de notification garde souvent un moteur WebView complet en mémoire (100 à 400 Mo). Prism le ferme quand l'appli n'a plus aucune fenêtre depuis le délai choisi ; elle le recrée quand vous la rouvrez (son contenu se recharge). Jamais pour un jeu, un anti-cheat ou un compagnon de jeu.",
            )
            .small()
            .color(th::muted()),
        );
        let before = self.webview.clone();
        let r = &mut self.webview;
        let new = &mut self.webview_new;
        card(ui, false, |ui| {
            ui.checkbox(
                &mut r.actif,
                RichText::new("Fermer les WebView en arrière-plan").strong(),
            );
            ui.add_enabled_ui(r.actif, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Après");
                    ui.add(egui::Slider::new(&mut r.minutes, 2..=60).suffix(" min sans fenêtre"));
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Jamais pour :").small().color(th::muted()));
                    let mut remove = None;
                    for (i, e) in r.exclus.iter().enumerate() {
                        if ui.small_button(format!("{e}  ✕")).on_hover_text("Retirer").clicked() {
                            remove = Some(i);
                        }
                    }
                    if let Some(i) = remove {
                        r.exclus.remove(i);
                    }
                    let edit = ui.add(
                        egui::TextEdit::singleline(new)
                            .hint_text("ms-teams.exe")
                            .desired_width(160.0),
                    );
                    let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if (ui.button("Ajouter").clicked() || enter) && !new.trim().is_empty() {
                        r.exclus.push(new.trim().to_lowercase());
                        new.clear();
                    }
                });
            });
        });
        if self.webview != before {
            *action = Some(self.backend.set_webview(&self.webview));
        }
    }

    fn services_page(&mut self, ui: &mut egui::Ui) {
        use prism_core::allege::StartType;
        // Liste lue en arrière-plan (plusieurs secondes sur certains PC).
        if self.services.is_none() && self.services_loading.is_none() {
            match self.factory {
                Some(factory) => {
                    let slot = std::sync::Arc::new(std::sync::Mutex::new(None));
                    let out = slot.clone();
                    std::thread::spawn(move || {
                        let r = factory().services();
                        if let Ok(mut o) = out.lock() {
                            *o = Some(r);
                        }
                    });
                    self.services_loading = Some(slot);
                }
                None => self.services = Some(self.backend.services()),
            }
        }
        if let Some(slot) = &self.services_loading {
            if let Some(r) = slot.lock().ok().and_then(|mut o| o.take()) {
                self.services = Some(r);
                self.services_loading = None;
            } else {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
            }
        }
        ui.label(
            RichText::new("Tous les services de Windows. Manuel = démarré par Windows quand un programme en a besoin (le plus sûr pour alléger) ; Désactivé = jamais. Les services dont les anti-cheats, les mises à jour et la sécurité ont besoin sont verrouillés. Chaque changement se remet d'un clic (↺) ou avec « Tout restaurer » de la page Allègement.")
                .color(th::muted()),
        );
        ui.add_space(8.0);
        let mut slim = false;
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.services_search)
                    .hint_text("Rechercher un service")
                    .desired_width(280.0),
            );
            ui.checkbox(&mut self.services_running_only, "En marche seulement");
            ui.checkbox(&mut self.services_superflus_only, "Superflus seulement")
                .on_hover_text("Services que Prism juge inutiles : télémétrie, diagnostics, mises à jour d'éditeurs tiers…");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .button("Désactiver tous les superflus")
                    .on_hover_text("Les passe à la demande ou désactivés (protégés exclus) et les arrête. Réversible : ↺ ou « Tout restaurer ».")
                    .clicked()
                {
                    slim = true;
                }
            });
        });
        ui.add_space(8.0);
        let rows = match &self.services {
            Some(Ok(r)) => r.clone(),
            Some(Err(e)) => {
                ui.label(RichText::new(format!("Liste des services illisible : {e}")).color(th::warn()));
                return;
            }
            None => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new("Lecture des services…").color(th::muted()));
                });
                return;
            }
        };
        let q = self.services_search.to_lowercase();
        let shown: Vec<&crate::backend::ServiceRow> = rows
            .iter()
            .filter(|r| !self.services_running_only || r.info.running)
            .filter(|r| !self.services_superflus_only || r.superflu.is_some())
            .filter(|r| {
                q.is_empty() || r.info.display.to_lowercase().contains(&q) || r.info.name.to_lowercase().contains(&q)
            })
            .collect();
        let running = rows.iter().filter(|r| r.info.running).count();
        ui.label(
            RichText::new(format!(
                "{} services · {running} en marche · {} affichés",
                rows.len(),
                shown.len()
            ))
            .small()
            .color(th::muted()),
        );
        let mut set: Option<(String, StartType)> = None;
        let mut restore: Option<String> = None;
        // Seules les lignes visibles sont dessinées (274 services : les dessiner toutes
        // à chaque image figeait la page, et faisait rater des clics).
        let row_h = 34.0;
        card(ui, false, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("services-rows")
                .max_height(560.0)
                .auto_shrink([false, true])
                .show_rows(ui, row_h, shown.len(), |ui, range| {
                    for r in &shown[range] {
                        ui.allocate_ui_with_layout(
                            egui::vec2(ui.available_width(), row_h),
                            Layout::left_to_right(Align::Center),
                            |ui| {
                                // Pastille dessinée (la police n'a pas le glyphe ●).
                                let (rect, resp) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), Sense::hover());
                                if r.info.running {
                                    ui.painter().circle_filled(rect.center(), 4.5, th::ok());
                                } else {
                                    ui.painter()
                                        .circle_stroke(rect.center(), 4.0, egui::Stroke::new(1.2, th::muted()));
                                }
                                resp.on_hover_text(if r.info.running { "en marche" } else { "arrêté" });
                                let name_w = (ui.available_width() - 330.0).max(160.0);
                                let label = ui
                                    .allocate_ui_with_layout(
                                        egui::vec2(name_w, row_h),
                                        Layout::left_to_right(Align::Center),
                                        |ui| {
                                            ui.set_min_width(name_w);
                                            ui.add(egui::Label::new(&r.info.display).truncate())
                                        },
                                    )
                                    .inner;
                                match &r.superflu {
                                    Some((why, _)) => {
                                        label.on_hover_text(format!("{} — superflu : {why}", r.info.name))
                                    }
                                    None => label.on_hover_text(&r.info.name),
                                };
                                let current = r.info.start;
                                let locked = r.protected.is_some()
                                    || r.info.per_user
                                    || current.is_none()
                                    || matches!(current, Some(StartType::Boot | StartType::System));
                                if locked {
                                    let why = match (&r.protected, r.info.per_user) {
                                        (Some(p), _) => format!("Verrouillé : {p}"),
                                        (None, true) => "Service par utilisateur : réglé par son modèle".to_string(),
                                        _ => "Service du noyau : non réglable".to_string(),
                                    };
                                    ui.add_space(48.0);
                                    ui.label(
                                        RichText::new(format!("🔒 {}", current.map(|c| c.label_fr()).unwrap_or("?")))
                                            .color(th::muted()),
                                    )
                                    .on_hover_text(why);
                                    return;
                                }
                                // Interrupteur : coupé = désactivé (et arrêté) ; rallumé =
                                // réglage d'origine, ou à la demande s'il était désactivé avant.
                                let mut on = current != Some(StartType::Disabled);
                                if ui
                                    .add(toggle_switch(&mut on))
                                    .on_hover_text("Activé / désactivé")
                                    .changed()
                                {
                                    if on && r.by_prism {
                                        restore = Some(r.info.name.clone());
                                    } else {
                                        set = Some((
                                            r.info.name.clone(),
                                            if on { StartType::Manual } else { StartType::Disabled },
                                        ));
                                    }
                                }
                                let mut sel = current.unwrap_or(StartType::Manual);
                                egui::ComboBox::from_id_salt(("svc", &r.info.name))
                                    .selected_text(sel.label_fr())
                                    .width(140.0)
                                    .show_ui(ui, |ui| {
                                        for st in [
                                            StartType::Auto,
                                            StartType::AutoDelayed,
                                            StartType::Manual,
                                            StartType::Disabled,
                                        ] {
                                            ui.selectable_value(&mut sel, st, st.label_fr());
                                        }
                                    });
                                if Some(sel) != current {
                                    set = Some((r.info.name.clone(), sel));
                                }
                                if r.by_prism
                                    && ui
                                        .small_button("↺")
                                        .on_hover_text("Remettre le réglage d'origine")
                                        .clicked()
                                {
                                    restore = Some(r.info.name.clone());
                                }
                            },
                        );
                    }
                });
        });
        if slim {
            self.bg("Services superflus", |b| b.services_slim());
        }
        if let Some((name, to)) = set {
            self.bg("Service", move |b| b.service_set(&name, to));
        }
        if let Some(name) = restore {
            self.bg("Service : origine", move |b| b.service_restore(&name));
        }
    }

    /// Plan automatique des jeux à anti-cheat noyau : tout est actif par défaut, chaque
    /// élément se coupe ici.
    fn noyau_section(&mut self, ui: &mut egui::Ui, action: &mut Option<Result<String, String>>) {
        section(ui, "Jeux à anti-cheat noyau");
        ui.label(
            RichText::new(
                "Valorant, Call of Duty, Fortnite, Battlefield, Apex, jeux FACEIT… Quand le Mode Jeu démarre et qu'un anti-cheat noyau est détecté, Prism prépare la machine tout seul, puis remet tout à la fin de la partie. Rien n'est injecté, le jeu et l'anti-cheat ne sont jamais touchés.",
            )
            .small()
            .color(th::muted()),
        );
        let before = self.noyau.clone();
        let r = &mut self.noyau;
        card(ui, false, |ui| {
            ui.checkbox(&mut r.actif, RichText::new("Automatique (recommandé)").strong());
            ui.add_enabled_ui(r.actif, |ui| {
                ui.checkbox(
                    &mut r.services_extreme,
                    "Remettre les services du niveau Extrême pendant la partie",
                );
                ui.checkbox(
                    &mut r.fermer_outils_genants,
                    "Fermer les outils qui bloquent les anti-cheats (débogueurs, Cheat Engine, System Informer)",
                );
                ui.checkbox(
                    &mut r.fermer_tous_les_outils,
                    "Fermer aussi les autres outils (Wireshark, machines virtuelles : travail en cours perdu)",
                );
                ui.checkbox(
                    &mut r.arreter_services_outils,
                    "Arrêter les services et pilotes des outils (Npcap, VMware, pilotes Sysinternals), relancés après",
                );
                ui.checkbox(
                    &mut r.eteindre_wsl,
                    "Éteindre WSL / Kali (rend la mémoire de sa machine virtuelle)",
                );
                ui.add_space(6.0);
                ui.label(RichText::new("Outils jamais touchés :").small().color(th::muted()));
                let tools = prism_core::config::Config::builtin().tools;
                ui.horizontal_wrapped(|ui| {
                    for t in tools
                        .iter()
                        .filter(|t| !t.processes.is_empty() || !t.services.is_empty())
                    {
                        let mut kept = r.exclus.contains(&t.id);
                        if ui.checkbox(&mut kept, &t.name).changed() {
                            if kept {
                                r.exclus.push(t.id.clone());
                            } else {
                                r.exclus.retain(|x| x != &t.id);
                            }
                        }
                    }
                });
            });
        });
        if self.noyau != before {
            *action = Some(self.backend.set_noyau(&self.noyau));
        }
    }

    // --- Apparence --------------------------------------------------------------

    fn appearance_page(&mut self, ui: &mut egui::Ui) {
        let mut action: Option<Result<String, String>> = None;
        ui.label(
            RichText::new("Réglages officiels de Windows et effets dessinés par Prism dans sa propre couche, tous réversibles. Rien n'est injecté dans les applis ni dans le compositeur de Windows : compatible avec les anti-cheats.")
                .color(th::muted()),
        );
        ui.add_space(10.0);
        self.bar_section(ui, &mut action);
        ui.add_space(12.0);
        section(ui, "Effets de Windows : préréglages");
        let presets = self.backend.appearance_presets();
        let n = presets.len() + 1;
        row(ui, n, |i, col| {
            if let Some(p) = presets.get(i) {
                let resp = card(col, false, |ui| {
                    ui.label(RichText::new(&p.label).size(17.0).strong());
                    ui.label(RichText::new(&p.description).small().color(th::muted()));
                });
                if resp.interact(Sense::click()).clicked() {
                    {
                        let id = p.id.clone();
                        self.bg("Apparence", move |b| b.appearance_preset(&id));
                    }
                }
            } else {
                let resp = card(col, false, |ui| {
                    ui.label(RichText::new("Réglages d'origine").size(17.0).strong());
                    ui.label(
                        RichText::new("Remet exactement ce que Windows avait avant Prism.")
                            .small()
                            .color(th::muted()),
                    );
                });
                if resp.interact(Sense::click()).clicked() {
                    self.bg("Apparence : restauration", |b| b.appearance_restore());
                }
            }
        });
        let rows = self.backend.appearance();
        let mut groups: Vec<String> = Vec::new();
        for r in &rows {
            if !groups.contains(&r.group) {
                groups.push(r.group.clone());
            }
        }
        for g in groups {
            ui.add_space(12.0);
            section(ui, &g);
            card(ui, false, |ui| {
                egui::Grid::new(format!("app-{g}"))
                    .num_columns(2)
                    .spacing([24.0, 10.0])
                    .show(ui, |ui| {
                        for r in rows.iter().filter(|r| r.group == g) {
                            ui.label(&r.label).on_hover_text(&r.why);
                            ui.horizontal(|ui| {
                                for (i, opt) in r.options.iter().enumerate() {
                                    let selected = r.current == Some(i);
                                    let text =
                                        RichText::new(opt).color(if selected { th::on_accent() } else { th::text() });
                                    let b = egui::Button::new(text).fill(if selected {
                                        th::accent()
                                    } else {
                                        th::card_hi()
                                    });
                                    if ui.add(b).clicked() && !selected {
                                        action = Some(self.backend.appearance_set(&r.id, i));
                                    }
                                }
                            });
                            ui.end_row();
                        }
                    });
            });
        }
        if let Some(r) = action {
            self.result(r);
        }
    }

    fn bar_section(&mut self, ui: &mut egui::Ui, action: &mut Option<Result<String, String>>) {
        use prism_core::bar::{
            DeskKind, Edge, OpacityRule, Widget, MARGIN_MAX, OPACITY_MIN, RULE_OPACITY_MIN, THICKNESS_MAX,
            THICKNESS_MIN,
        };
        let running = self.backend.bar_running();
        let mut cfg = self.backend.bar_config();
        let before = cfg.clone();
        self.theme_section(ui, &mut cfg.theme);
        ui.add_space(12.0);
        tiling_section(ui, &mut cfg.tiling);
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            section(ui, "Prism Bar");
            ui.label(
                RichText::new(
                    "Votre barre des tâches : n'importe quel bord, taille et opacité au choix, widgets système.",
                )
                .small()
                .color(th::muted()),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if running {
                    if ui.button("Arrêter la barre").clicked() {
                        *action = Some(self.backend.bar_stop());
                    }
                    pill(ui, "✔ en marche", th::ok());
                } else if ui
                    .add(egui::Button::new(RichText::new("Lancer la barre").color(th::on_accent())).fill(th::accent()))
                    .clicked()
                {
                    *action = Some(self.backend.bar_start());
                }
            });
        });
        let new_rule = &mut self.new_rule;
        card(ui, false, |ui| {
            ui.horizontal_top(|ui| {
                let preview_w = (ui.available_width() * 0.42).min(460.0);
                ui.allocate_ui_with_layout(Vec2::new(preview_w, 0.0), Layout::top_down(Align::Min), |ui| {
                    ui.set_width(preview_w);
                    bar_preview(ui, &cfg);
                });
                ui.add_space(16.0);
                // L'espacement automatique entre éléments s'ajoute aux 16 px : sans le
                // retirer, la colonne déborde de la carte (vu sur capture).
                let rest = ui.available_width() - ui.spacing().item_spacing.x * 2.0;
                ui.allocate_ui_with_layout(Vec2::new(rest, 0.0), Layout::top_down(Align::Min), |ui| {
                    ui.set_width(rest);
                    let field = |ui: &mut egui::Ui, name: &str| {
                        ui.add_space(4.0);
                        ui.label(RichText::new(name).small().color(th::muted()));
                    };
                    field(ui, "Position");
                    ui.horizontal(|ui| {
                        for e in Edge::ALL {
                            let sel = cfg.edge == e;
                            let b = egui::Button::new(RichText::new(e.label()).color(if sel {
                                th::on_accent()
                            } else {
                                th::text()
                            }))
                            .fill(if sel { th::accent() } else { th::card_hi() });
                            if ui.add(b).clicked() {
                                cfg.edge = e;
                            }
                        }
                    });
                    field(ui, "Taille et transparence");
                    ui.spacing_mut().slider_width = (rest * 0.45).clamp(120.0, 260.0);
                    ui.add(
                        egui::Slider::new(&mut cfg.thickness, THICKNESS_MIN..=THICKNESS_MAX)
                            .text("épaisseur")
                            .suffix(" px"),
                    );
                    ui.add(
                        egui::Slider::new(&mut cfg.margin, 0..=MARGIN_MAX)
                            .text("marge (barre flottante)")
                            .suffix(" px"),
                    );
                    ui.add(
                        egui::Slider::new(&mut cfg.opacity, OPACITY_MIN..=100)
                            .text("opacité")
                            .suffix(" %"),
                    );
                    field(ui, "Options");
                    ui.horizontal_wrapped(|ui| {
                        ui.checkbox(&mut cfg.rounded, "Coins arrondis");
                        ui.checkbox(&mut cfg.hide_windows_taskbar, "Masquer la barre Windows");
                        ui.checkbox(&mut cfg.prism_start_menu, "Menu Démarrer de Prism (Alt+F1)")
                            .on_hover_text("Le bouton Démarrer ouvre le menu de Prism : recherche, applis épinglées (clic droit), alimentation. Décoché : le menu de Windows.");
                        ui.checkbox(&mut cfg.hide_in_fullscreen, "Se cacher en plein écran");
                        ui.checkbox(&mut cfg.prism_icon, "Icône Prism dans la zone de notification")
                            .on_hover_text("Dans les icônes cachées de Windows (et de la zone système de la barre) : clic pour ouvrir Prism, clic droit pour libérer la RAM.");
                    });
                    field(ui, "Plusieurs écrans");
                    ui.horizontal_wrapped(|ui| {
                        ui.checkbox(&mut cfg.all_monitors, "Une barre sur chaque écran");
                        ui.add_enabled(
                            cfg.all_monitors,
                            egui::Checkbox::new(
                                &mut cfg.windows_per_monitor,
                                "Chaque barre montre les fenêtres de son écran",
                            ),
                        );
                    });
                    field(ui, "Widgets de la barre");
                    ui.horizontal_wrapped(|ui| {
                        for w in Widget::ALL {
                            let mut on = cfg.widgets.contains(&w);
                            if ui.checkbox(&mut on, w.label()).changed() {
                                if on {
                                    cfg.widgets.push(w);
                                } else {
                                    cfg.widgets.retain(|x| *x != w);
                                }
                                // Ordre stable : celui de la liste de référence.
                                cfg.widgets.sort_by_key(|x| Widget::ALL.iter().position(|y| y == x));
                            }
                        }
                    });
                    field(ui, "Widgets du bureau (à déplacer à la souris, place retenue)");
                    ui.horizontal_wrapped(|ui| {
                        for k in DeskKind::ALL {
                            let mut on = cfg.desktop_widgets.iter().any(|w| w.kind == k);
                            if ui.checkbox(&mut on, k.label()).changed() {
                                cfg.toggle_desktop(k, on);
                            }
                        }
                    });
                    ui.add(
                        egui::Slider::new(&mut cfg.desktop_opacity, OPACITY_MIN..=100)
                            .text("opacité des widgets")
                            .suffix(" %"),
                    );
                    field(
                        ui,
                        "Transparence des éléments (toutes les applis classiques, jamais les jeux)",
                    );
                    for k in prism_core::bar::ElementKind::ALL {
                        ui.add(
                            egui::Slider::new(cfg.element_opacity.get_mut(k), RULE_OPACITY_MIN..=100)
                                .text(k.label())
                                .suffix(" %"),
                        );
                    }
                    field(ui, "Transparence des applis (jamais sur un jeu ni en plein écran)");
                    let mut remove = None;
                    for (i, r) in cfg.opacity_rules.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&r.process).monospace());
                            ui.add(egui::Slider::new(&mut r.opacity, RULE_OPACITY_MIN..=100).suffix(" %"));
                            if ui.small_button("✖").on_hover_text("Retirer la règle").clicked() {
                                remove = Some(i);
                            }
                        });
                    }
                    if let Some(i) = remove {
                        cfg.opacity_rules.remove(i);
                    }
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(new_rule)
                                .hint_text("windowsterminal.exe")
                                .desired_width(200.0),
                        );
                        if ui.button("Ajouter").clicked() && !new_rule.trim().is_empty() {
                            cfg.opacity_rules.push(OpacityRule {
                                process: new_rule.trim().to_lowercase(),
                                opacity: 90,
                            });
                            new_rule.clear();
                        }
                    });
                });
            });
        });
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            section(ui, "Effets de fenêtres");
            ui.label(
                RichText::new("Lampe de génie, gélatine, zoom : dessinés par Prism (la Prism Bar doit tourner), jamais sur un jeu ni en plein écran.")
                    .small()
                    .color(th::muted()),
            );
        });
        let was_enabled = cfg.fx.enabled;
        card(ui, false, |ui| {
            ui.checkbox(&mut cfg.fx.enabled, "Activer les effets");
            ui.add_enabled_ui(cfg.fx.enabled, |ui| {
                egui::Grid::new("fx")
                    .num_columns(2)
                    .spacing([16.0, 8.0])
                    .show(ui, |ui| {
                        let rows: [(&str, &mut prism_core::fx::Effect); 4] = [
                            ("Réduire", &mut cfg.fx.minimize),
                            ("Restaurer", &mut cfg.fx.restore),
                            ("Ouvrir", &mut cfg.fx.open),
                            ("Fermer", &mut cfg.fx.close),
                        ];
                        for (label, value) in rows {
                            ui.label(label);
                            egui::ComboBox::from_id_salt(label)
                                .selected_text(value.label())
                                .width(180.0)
                                .show_ui(ui, |ui| {
                                    for e in prism_core::fx::Effect::ALL {
                                        ui.selectable_value(value, e, e.label());
                                    }
                                });
                            ui.end_row();
                        }
                        ui.label("Agrandir / ancrer");
                        egui::ComboBox::from_id_salt("maximize")
                            .selected_text(cfg.fx.maximize.label())
                            .width(180.0)
                            .show_ui(ui, |ui| {
                                for e in prism_core::fx_effects::MorphEffect::ALL {
                                    ui.selectable_value(&mut cfg.fx.maximize, e, e.label());
                                }
                            });
                        ui.end_row();
                    });
                ui.add(
                    egui::Slider::new(
                        &mut cfg.fx.duration_ms,
                        prism_core::fx::DURATION_MIN..=prism_core::fx::DURATION_MAX,
                    )
                    .text("durée")
                    .suffix(" ms"),
                );
                ui.add(
                    egui::Slider::new(&mut cfg.fx.intensity, 0..=100)
                        .text("intensité de la déformation")
                        .suffix(" %"),
                );
                ui.checkbox(
                    &mut cfg.fx.drag,
                    "Gélatine pendant le déplacement : la fenêtre ondule quand on la fait glisser",
                );
            });
        });
        if cfg.fx.enabled && !was_enabled {
            // Coupe l'animation de réduction de Windows pour ne pas la superposer à celle
            // de Prism (réglage d'Apparence journalisé : « Réglages d'origine » la remet).
            if let Some(row) = self.backend.appearance().into_iter().find(|r| r.id == "anim_minmax") {
                if let Some(i) = row.options.iter().position(|o| o == "Instantanée") {
                    let _ = self.backend.appearance_set("anim_minmax", i);
                }
            }
        }
        if cfg != before {
            if let Err(e) = self.backend.set_bar_config(&cfg) {
                *action = Some(Err(e));
            }
            if cfg.theme != before.theme {
                th::set_theme(ui.ctx(), &cfg.theme);
            }
        }
    }

    /// Thèmes de couleurs : la barre, les widgets et l'appli changent ensemble.
    fn theme_section(&mut self, ui: &mut egui::Ui, theme: &mut prism_core::theme::ThemeConfig) {
        use prism_core::theme::{ThemeConfig, PRESETS};
        let c = |rgb: [u8; 3]| egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
        ui.horizontal(|ui| {
            section(ui, "Thème");
            ui.label(
                RichText::new("Couleurs de la Prism Bar, des widgets et de l'appli.")
                    .small()
                    .color(th::muted()),
            );
        });
        card(ui, false, |ui| {
            ui.horizontal_wrapped(|ui| {
                for p in &PRESETS {
                    let pal = ThemeConfig {
                        preset: p.id.into(),
                        colors: Default::default(),
                        accent: None,
                    }
                    .palette();
                    let selected = theme.preset == p.id;
                    let (rect, resp) = ui.allocate_exact_size(egui::vec2(132.0, 56.0), egui::Sense::click());
                    let painter = ui.painter_at(rect);
                    painter.rect_filled(rect, 8.0, c(pal.bg));
                    let bar = egui::Rect::from_min_size(
                        egui::pos2(rect.left() + 10.0, rect.bottom() - 16.0),
                        egui::vec2(rect.width() - 20.0, 6.0),
                    );
                    painter.rect_filled(bar, 3.0, c(pal.card_hi));
                    painter.rect_filled(
                        egui::Rect::from_min_size(bar.min, egui::vec2(bar.width() * 0.45, 6.0)),
                        3.0,
                        c(pal.accent),
                    );
                    painter.text(
                        egui::pos2(rect.left() + 10.0, rect.top() + 10.0),
                        egui::Align2::LEFT_TOP,
                        p.label,
                        egui::FontId::proportional(13.0),
                        c(pal.text),
                    );
                    let stroke = if selected {
                        egui::Stroke::new(2.0, c(pal.accent))
                    } else if resp.hovered() {
                        egui::Stroke::new(1.0, th::muted())
                    } else {
                        egui::Stroke::new(1.0, th::border())
                    };
                    painter.rect_stroke(rect, 8.0, stroke, egui::StrokeKind::Inside);
                    if resp.clicked() {
                        theme.preset = p.id.into();
                    }
                }
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let mut custom = theme.accent.is_some();
                if ui.checkbox(&mut custom, "Accent personnalisé").changed() {
                    theme.accent = custom.then(|| theme.palette().accent);
                }
                if let Some(a) = theme.accent.as_mut() {
                    ui.color_edit_button_srgb(a);
                    ui.label(
                        RichText::new("le texte posé sur l'accent s'adapte pour rester lisible")
                            .small()
                            .color(th::muted()),
                    );
                }
            });
            // Chaque couleur au choix, en plus des thèmes.
            ui.add_space(6.0);
            egui::CollapsingHeader::new(RichText::new("Couleurs personnalisées").strong())
                .id_salt("couleurs-perso")
                .show(ui, |ui| {
                    let pal = theme.palette();
                    egui::Grid::new("couleurs")
                        .num_columns(6)
                        .spacing([10.0, 8.0])
                        .show(ui, |ui| {
                            for (i, (role, label)) in prism_core::theme::ROLES.iter().enumerate() {
                                let mut col = pal.get(role).unwrap_or([0, 0, 0]);
                                if ui.color_edit_button_srgb(&mut col).changed() {
                                    theme.colors.insert(role.to_string(), col);
                                }
                                let custom = theme.colors.contains_key(*role);
                                ui.label(if custom {
                                    RichText::new(*label).strong()
                                } else {
                                    RichText::new(*label)
                                });
                                if i % 3 == 2 {
                                    ui.end_row();
                                }
                            }
                        });
                    if !theme.colors.is_empty() && ui.button("Revenir aux couleurs du thème").clicked() {
                        theme.colors.clear();
                    }
                });
        });

        // Tout Windows aux couleurs du thème.
        ui.add_space(10.0);
        section(ui, "Tout Windows aux couleurs du thème");
        let mut apply = false;
        let mut restore = false;
        card(ui, false, |ui| {
            ui.label(
                RichText::new("Réglages officiels de Windows, tous réversibles : la couleur d'accent (barres de titre, bordures, sélections, menu Démarrer de Windows), un fond d'écran futuriste généré aux couleurs du thème, le mode sombre.")
                    .small()
                    .color(th::muted()),
            );
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut self.design.0, "Couleur d'accent de Windows");
                ui.checkbox(&mut self.design.1, "Fond d'écran du thème");
                ui.checkbox(&mut self.design.2, "Mode sombre");
                ui.checkbox(&mut self.design.3, "Barres de titre colorées");
            });
            ui.horizontal(|ui| {
                if ui.button("Appliquer le thème à tout Windows").clicked() {
                    apply = true;
                }
                if ui.button("Remettre Windows comme avant").clicked() {
                    restore = true;
                }
            });
        });
        if apply {
            let (a, f, d, t) = self.design;
            self.bg("Design de Windows", move |b| b.windows_design_apply(a, f, d, t));
        }
        if restore {
            self.bg("Design de Windows : restauration", |b| b.windows_design_restore());
        }
    }

    // --- Outils -----------------------------------------------------------------

    fn tools_page(&mut self, ui: &mut egui::Ui) {
        if !self.tools_scan_asked {
            self.backend.tools_scan();
            self.tools_scan_asked = true;
        }
        let view = self.backend.tools_view();
        let busy = view.scanning || view.job.as_ref().is_some_and(|j| !j.done);
        if busy {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(400));
        }
        ui.label(
            RichText::new("Rien n'est installé par défaut. Installation et désinstallation se font ici, sans console. Pendant un jeu à anti-cheat noyau, les outils passent en veille tout seuls (page Allègement).")
                .color(th::muted()),
        );
        ui.add_space(8.0);
        if let Some(j) = &view.job {
            card(ui, false, |ui| {
                if !j.done {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(
                            RichText::new(format!(
                                "{} en cours : {} ({}/{})",
                                if j.install { "Installation" } else { "Désinstallation" },
                                j.current,
                                j.step,
                                j.total
                            ))
                            .strong(),
                        );
                    });
                    ui.label(RichText::new("Vous pouvez continuer à utiliser Prism. Un outil peut prendre plusieurs minutes (téléchargement).").small().color(th::muted()));
                } else {
                    ui.label(
                        RichText::new(format!(
                            "{} terminée : {} réussi(s), {} échec(s)",
                            if j.install { "Installation" } else { "Désinstallation" },
                            j.ok.len(),
                            j.errors.len()
                        ))
                        .strong()
                        .color(if j.errors.is_empty() { th::ok() } else { th::warn() }),
                    );
                }
                for e in &j.errors {
                    ui.label(RichText::new(e).small().color(th::warn()));
                }
            });
            ui.add_space(8.0);
        }
        let packs = self.backend.packs();
        let cfg = prism_core::config::Config::builtin();
        let mut run: Option<(bool, Vec<String>)> = None;
        let installed = |id: &str| view.installed.iter().any(|x| x == id);
        row(ui, 2, |c, col| {
            for p in packs.iter().skip(c).step_by(2) {
                card(col, false, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&p.label).size(17.0).strong());
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.add_enabled_ui(!busy, |ui| {
                                let present: Vec<String> = p
                                    .tools
                                    .iter()
                                    .filter(|t| installed(&t.0))
                                    .map(|t| t.0.clone())
                                    .collect();
                                if !present.is_empty() && ui.button("Tout désinstaller").clicked() {
                                    run = Some((false, present));
                                }
                                if p.tools.iter().any(|t| !installed(&t.0)) && ui.button("Tout installer").clicked() {
                                    run = Some((true, p.tools.iter().map(|t| t.0.clone()).collect()));
                                }
                            });
                        });
                    });
                    for (id, name, warn) in &p.tools {
                        let on = installed(id);
                        ui.horizontal(|ui| {
                            let (mark, color) = if !view.scanned {
                                ("…", th::muted())
                            } else if on {
                                ("✔", th::ok())
                            } else {
                                ("·", th::muted())
                            };
                            ui.label(RichText::new(mark).color(color));
                            ui.label(name);
                            if let Some(w) = warn {
                                ui.label(RichText::new("⚠ anti-cheat").small().color(th::warn()))
                                    .on_hover_text(w);
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.add_enabled_ui(!busy && view.scanned, |ui| {
                                    let label = if on { "Désinstaller" } else { "Installer" };
                                    if ui.small_button(label).clicked() {
                                        run = Some((!on, vec![id.clone()]));
                                    }
                                });
                            });
                        });
                    }
                });
                col.add_space(10.0);
            }
        });
        if view.scanning {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(
                    RichText::new("Lecture des outils installés…")
                        .small()
                        .color(th::muted()),
                );
            });
        }
        if let Some((install, ids)) = run {
            let erases = !install
                && ids
                    .iter()
                    .filter_map(|id| cfg.tool(id))
                    .any(prism_core::tools::uninstall_erases_data);
            if erases {
                self.tools_confirm = Some(ids);
            } else {
                let r = self.backend.tools_run(install, ids);
                self.result(r);
            }
        }
        if let Some(ids) = self.tools_confirm.clone() {
            let mut close = false;
            egui::Window::new("Effacer Kali Linux ?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ui.ctx(), |ui| {
                    ui.label("Désinstaller Kali Linux efface la distribution WSL et TOUS les fichiers qu'elle contient (dossier personnel, outils installés, résultats).");
                    ui.label(RichText::new("Pour seulement libérer la mémoire pendant un jeu, inutile : Prism éteint WSL tout seul.").small().color(th::muted()));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Effacer et désinstaller").clicked() {
                            let r = self.backend.tools_run(false, ids.clone());
                            self.result(r);
                            close = true;
                        }
                        if ui.button("Annuler").clicked() {
                            close = true;
                        }
                    });
                });
            if close {
                self.tools_confirm = None;
            }
        }
    }
}

impl eframe::App for PrismApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }
}

fn debug_frames(ctx: &egui::Context, now: f64) {
    thread_local! {
        static FRAMES: std::cell::Cell<(u32, f64)> = const { std::cell::Cell::new((0, 0.0)) };
    }
    let causes = ctx.repaint_causes();
    FRAMES.with(|f| {
        let (n, since) = f.get();
        if now - since >= 1.0 {
            let line = format!(
                "{now:.1}s {} images/s · causes : {:?}\n",
                n + 1,
                causes.iter().map(|c| c.to_string()).collect::<Vec<_>>()
            );
            let path = prism_core::paths::user_dir().join("ui-debug.log");
            if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
                use std::io::Write;
                let _ = file.write_all(line.as_bytes());
            }
            f.set((0, now));
        } else {
            f.set((n + 1, since));
        }
    });
}

// --- Briques visuelles --------------------------------------------------------

/// Nom lisible d'une entrée de démarrage : pour une appli du Store,
/// `MSTeams_8wekyb3d8bbwe\TeamsTfwStartupTask` devient « MSTeams » (le nom
/// technique reste en info-bulle).
fn display_name(r: &StartupRow) -> (String, Option<String>) {
    if r.source == prism_core::demarrage::Source::StoreTask {
        let family = r.name.split('\\').next().unwrap_or(&r.name);
        let short = family.split('_').next().unwrap_or(family);
        let short = short.rsplit('.').next().unwrap_or(short);
        return (short.to_string(), Some(r.name.clone()));
    }
    (r.name.clone(), None)
}

/// `n` colonnes de même largeur, alignées en haut (largeurs explicites : aucune
/// colonne ne peut déborder sur sa voisine).
fn row(ui: &mut egui::Ui, n: usize, mut add: impl FnMut(usize, &mut egui::Ui)) {
    let gap = 12.0;
    let w = ((ui.available_width() - gap * (n as f32 - 1.0)) / n as f32).floor();
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        for i in 0..n {
            ui.allocate_ui_with_layout(Vec2::new(w, 0.0), Layout::top_down(Align::Min), |ui| {
                ui.set_width(w);
                add(i, ui);
            });
        }
    });
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.label(RichText::new(title.to_uppercase()).small().strong().color(th::muted()));
    ui.add_space(2.0);
}

fn stat_title(ui: &mut egui::Ui, title: &str) {
    ui.label(RichText::new(title).small().color(th::accent()));
}

/// Carte en relief, interactive (voir `futur::card3d`) : toutes les pages en profitent.
fn card(ui: &mut egui::Ui, highlighted: bool, add: impl FnOnce(&mut egui::Ui)) -> egui::Response {
    crate::futur::card3d(ui, highlighted, add)
}

fn pill(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.15))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::symmetric(10, 4))
        .show(ui, |ui| {
            ui.add(egui::Label::new(RichText::new(text).small().color(color)).wrap_mode(egui::TextWrapMode::Extend));
        });
}

fn chip(ui: &mut egui::Ui, text: &str) {
    // Un simple libellé surligné : il se replie correctement dans une rangée.
    ui.add(
        egui::Label::new(
            RichText::new(format!(" {text} "))
                .small()
                .background_color(th::card_hi()),
        )
        .wrap_mode(egui::TextWrapMode::Extend),
    );
}

fn badge(ui: &mut egui::Ui, advice: &str) {
    let color = match advice {
        "à désactiver" => th::warn(),
        "protégé" => th::ok(),
        "à garder" => th::ok(),
        "optionnel" => th::accent(),
        _ => th::muted(),
    };
    pill(ui, advice, color);
}

fn class_color(class: &str) -> Color32 {
    match class {
        "jeu" => th::accent(),
        "arrière-plan" => th::warn(),
        "compagnon" => th::ok(),
        _ => th::muted(),
    }
}

fn bar(ui: &mut egui::Ui, fraction: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 8.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(4), th::card_hi());
    let mut fill = rect;
    fill.set_width(rect.width() * fraction.clamp(0.0, 1.0));
    let color = if fraction > 0.85 {
        th::bad()
    } else if fraction > 0.7 {
        th::warn()
    } else {
        th::accent()
    };
    p.rect_filled(fill, CornerRadius::same(4), color);
}

fn store_color(s: Store) -> Color32 {
    match s {
        Store::Steam => Color32::from_rgb(0x2a, 0x75, 0xbb),
        Store::Epic => Color32::from_rgb(0x9a, 0x9a, 0x9a),
        Store::Gog => Color32::from_rgb(0x86, 0x3c, 0xd8),
        Store::BattleNet => Color32::from_rgb(0x14, 0x8e, 0xff),
    }
}

fn game_tile(ui: &mut egui::Ui, g: &Game, size: Vec2, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hovered = resp.hovered() || selected;
    let p = ui.painter();
    p.rect_filled(
        rect,
        CornerRadius::same(12),
        if hovered { th::card_hi() } else { th::card() },
    );
    p.rect_stroke(
        rect,
        CornerRadius::same(12),
        Stroke::new(
            if selected { 2.5 } else { 1.0 },
            if hovered { th::accent() } else { th::border() },
        ),
        egui::StrokeKind::Inside,
    );
    let band = egui::Rect::from_min_size(rect.min, Vec2::new(rect.width(), 5.0));
    p.rect_filled(
        band,
        CornerRadius {
            nw: 12,
            ne: 12,
            sw: 0,
            se: 0,
        },
        store_color(g.store),
    );
    let initial = g.name.chars().next().unwrap_or('?').to_string();
    p.text(
        rect.left_top() + Vec2::new(16.0, 18.0),
        egui::Align2::LEFT_TOP,
        initial,
        egui::FontId::proportional(size.y * 0.32),
        store_color(g.store).gamma_multiply(0.9),
    );
    p.text(
        rect.left_bottom() + Vec2::new(16.0, -34.0),
        egui::Align2::LEFT_BOTTOM,
        &g.name,
        egui::FontId::proportional(if size.y > 140.0 { 19.0 } else { 15.0 }),
        th::text(),
    );
    p.text(
        rect.left_bottom() + Vec2::new(16.0, -14.0),
        egui::Align2::LEFT_BOTTOM,
        g.store.label(),
        egui::FontId::proportional(12.0),
        th::muted(),
    );
    if hovered {
        p.text(
            rect.right_bottom() + Vec2::new(-16.0, -14.0),
            egui::Align2::RIGHT_BOTTOM,
            "▶ Jouer",
            egui::FontId::proportional(13.0),
            th::accent(),
        );
    }
    resp
}

/// Aperçu de la barre sur un écran miniature, avec le même code de disposition que
/// la vraie barre (`prism_core::bar::layout`).
/// Fenêtres en tuiles : disposition, écarts, part de la principale, exclusions.
fn tiling_section(ui: &mut egui::Ui, t: &mut prism_core::tiling::TilingConfig) {
    use prism_core::tiling::{Layout, GAP_MAX, PERCENT_MAX, PERCENT_MIN};
    ui.horizontal(|ui| {
        section(ui, "Fenêtres en tuiles");
        ui.label(
            RichText::new("Les fenêtres se rangent seules côte à côte, écran par écran (la Prism Bar doit tourner).")
                .small()
                .color(th::muted()),
        );
    });
    card(ui, false, |ui| {
        ui.checkbox(&mut t.enabled, "Ranger les fenêtres en tuiles");
        ui.add_enabled_ui(t.enabled, |ui| {
            ui.horizontal(|ui| {
                ui.label("Disposition");
                for l in Layout::ALL {
                    let sel = t.layout == l;
                    let b = egui::Button::new(RichText::new(l.label()).color(if sel { th::on_accent() } else { th::text() }))
                        .fill(if sel { th::accent() } else { th::card_hi() });
                    if ui.add(b).clicked() {
                        t.layout = l;
                    }
                }
            });
            ui.add(egui::Slider::new(&mut t.gap, 0..=GAP_MAX).text("écart entre les fenêtres").suffix(" px"));
            ui.add_enabled(
                t.layout == Layout::MasterStack,
                egui::Slider::new(&mut t.master_percent, PERCENT_MIN..=PERCENT_MAX)
                    .text("largeur de la fenêtre principale")
                    .suffix(" %"),
            );
            ui.horizontal(|ui| {
                ui.label("Jamais en tuiles");
                let mut text = t.exclude.join(", ");
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut text)
                        .hint_text("ex. vlc.exe, *photoshop*")
                        .desired_width(320.0),
                );
                if resp.changed() {
                    t.exclude = text
                        .split(',')
                        .map(|x| x.trim().to_string())
                        .filter(|x| !x.is_empty())
                        .collect();
                }
            });
            ui.label(
                RichText::new(
                    "Jamais un jeu, une fenêtre plein écran, une boîte de dialogue ni une fenêtre agrandie. \
                     Une fenêtre qui ne tient pas dans sa tuile (taille minimale) reste flottante. \
                     Désactiver remet chaque fenêtre à sa place.",
                )
                .small()
                .color(th::muted()),
            );
            ui.label(
                RichText::new(
                    "Raccourcis : Win+Ctrl+Alt+W oui/non · Espace disposition suivante · Entrée fenêtre active en principale · \
                     flèches gauche/droite fenêtre précédente/suivante · flèches haut/bas largeur de la principale · F flottante/en tuile",
                )
                .small()
                .color(th::muted()),
            );
        });
    });
}

fn bar_preview(ui: &mut egui::Ui, cfg: &prism_core::bar::BarConfig) {
    use prism_core::bar::{bar_rect, layout, Rect as BRect, Widget};
    let w = ui.available_width().min(520.0);
    let h = w * 9.0 / 16.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
    let p = ui.painter();
    // Écran simulé en 1920x1080, puis réduit.
    let k = w / 1920.0;
    p.rect_filled(rect, CornerRadius::same(8), Color32::from_rgb(0x24, 0x3b, 0x55));
    p.rect_filled(
        rect.shrink(1.0),
        CornerRadius::same(8),
        Color32::from_rgb(0x1a, 0x2a, 0x3d),
    );
    // Une fenêtre factice dans la zone de travail.
    let (bar, reserved) = bar_rect(
        BRect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        },
        cfg,
    );
    let work = match cfg.edge {
        prism_core::bar::Edge::Top => {
            egui::Rect::from_min_max(egui::pos2(0.0, reserved.bottom as f32), egui::pos2(1920.0, 1080.0))
        }
        prism_core::bar::Edge::Bottom => {
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1920.0, reserved.top as f32))
        }
        prism_core::bar::Edge::Left => {
            egui::Rect::from_min_max(egui::pos2(reserved.right as f32, 0.0), egui::pos2(1920.0, 1080.0))
        }
        prism_core::bar::Edge::Right => {
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(reserved.left as f32, 1080.0))
        }
    };
    let to_screen = |x: f32, y: f32| rect.min + Vec2::new(x * k, y * k);
    let win = egui::Rect::from_min_max(
        to_screen(work.min.x + 160.0, work.min.y + 120.0),
        to_screen(work.max.x - 160.0, work.max.y - 120.0),
    );
    p.rect_filled(win, CornerRadius::same(4), Color32::from_rgb(0x2b, 0x36, 0x44));
    p.rect_filled(
        egui::Rect::from_min_size(win.min, Vec2::new(win.width(), 10.0)),
        CornerRadius::same(4),
        Color32::from_rgb(0x36, 0x44, 0x55),
    );
    // La barre.
    let alpha = (cfg.opacity as f32 / 100.0 * 255.0) as u8;
    let br = egui::Rect::from_min_max(
        to_screen(bar.left as f32, bar.top as f32),
        to_screen(bar.right as f32, bar.bottom as f32),
    );
    let radius = if cfg.rounded { 6 } else { 0 };
    // Couleurs du thème choisi (celui de la configuration, même avant d'être enregistré).
    let pal = cfg.theme.palette();
    let c = |rgb: [u8; 3]| Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
    let tint = |base: [u8; 3], to: [u8; 3]| c(prism_core::theme::mix(base, to, 0.35));
    p.rect_filled(
        br,
        CornerRadius::same(radius),
        Color32::from_rgba_unmultiplied(pal.bg[0], pal.bg[1], pal.bg[2], alpha),
    );
    for it in layout(cfg, bar.width(), bar.height(), 3) {
        let r = egui::Rect::from_min_max(
            br.min + Vec2::new(it.rect.left as f32 * k, it.rect.top as f32 * k),
            br.min + Vec2::new(it.rect.right as f32 * k, it.rect.bottom as f32 * k),
        )
        .shrink(1.5);
        let color = match it.widget {
            Widget::Start => c(pal.accent),
            Widget::Windows if it.index == Some(0) => c(pal.accent_dim),
            Widget::Windows => c(pal.card_hi),
            Widget::Cpu | Widget::Ram | Widget::Gpu => tint(pal.card, pal.ok),
            Widget::GameMode => tint(pal.card, pal.accent),
            Widget::Network => tint(pal.card, pal.warn),
            Widget::Tray | Widget::Clock | Widget::Overflow => c(pal.card_hi),
        };
        p.rect_filled(r, CornerRadius::same(2), color);
    }
}

/// Interrupteur (case à cocher redessinée).
fn toggle_switch(on: &mut bool) -> impl egui::Widget + '_ {
    move |ui: &mut egui::Ui| {
        let size = Vec2::new(38.0, 20.0);
        let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click());
        if resp.clicked() {
            *on = !*on;
            resp.mark_changed();
        }
        let t = ui.ctx().animate_bool_responsive(resp.id, *on);
        let enabled = ui.is_enabled();
        let bg = if *on { th::accent() } else { th::card_hi() };
        let p = ui.painter();
        p.rect_filled(
            rect,
            CornerRadius::same(10),
            if enabled { bg } else { bg.gamma_multiply(0.4) },
        );
        let x = egui::lerp(rect.left() + 10.0..=rect.right() - 10.0, t);
        p.circle_filled(
            egui::pos2(x, rect.center().y),
            7.0,
            if *on { th::on_accent() } else { th::muted() },
        );
        resp
    }
}
