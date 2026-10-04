//! L'interface : tableau de bord, jeux (et mode console), démarrage, allègement, outils.

use eframe::egui::{self, Align, Color32, CornerRadius, Layout, Margin, RichText, Sense, Stroke, Vec2};
use prism_core::allege::Tier;
use prism_core::library::{Game, Store};
use prism_core::model::human_bytes;

use crate::backend::{AllegeRow, Backend, Live, StartupRow};
use crate::theme::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Dashboard,
    Games,
    Startup,
    Allege,
    Appearance,
    Tools,
}

impl Page {
    const ALL: [Page; 6] = [
        Page::Dashboard,
        Page::Games,
        Page::Startup,
        Page::Allege,
        Page::Appearance,
        Page::Tools,
    ];

    fn label(self) -> &'static str {
        match self {
            Page::Dashboard => "Tableau de bord",
            Page::Games => "Jeux",
            Page::Startup => "Démarrage",
            Page::Allege => "Allègement",
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
            Page::Appearance => "🖥",
            Page::Tools => "🛡",
        }
    }
}

pub struct PrismApp {
    backend: Box<dyn Backend>,
    pub page: Page,
    live: Live,
    startup: Vec<StartupRow>,
    allege: Vec<AllegeRow>,
    games: Vec<Game>,
    toast: Option<(String, bool)>,
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
        let games = backend.games();
        PrismApp {
            backend,
            page: Page::Dashboard,
            live,
            startup,
            allege,
            games,
            toast: None,
            console: false,
            console_sel: 0,
            new_rule: String::new(),
            last_refresh: 0.0,
        }
    }

    fn result(&mut self, r: Result<String, String>) {
        self.toast = Some(match r {
            Ok(m) => (m, true),
            Err(e) => (e, false),
        });
        self.startup = self.backend.startup().unwrap_or_default();
        self.allege = self.backend.allege();
        self.live = self.backend.live();
    }

    /// Une frame, dans le `Ui` racine. Séparé de `eframe::App` pour les tests.
    pub fn show(&mut self, root: &mut egui::Ui) {
        let ctx = root.ctx().clone();
        let now = ctx.input(|i| i.time);
        if now - self.last_refresh > 1.0 {
            self.live = self.backend.live();
            self.last_refresh = now;
        }
        ctx.request_repaint_after(std::time::Duration::from_secs(1));

        if self.console {
            self.console_ui(root);
            return;
        }

        egui::Panel::left("nav")
            .exact_size(212.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(PANEL).inner_margin(Margin::symmetric(14, 18)))
            .show(root, |ui| self.nav(ui));

        egui::Panel::top("header")
            .exact_size(58.0)
            .frame(egui::Frame::new().fill(BG).inner_margin(Margin::symmetric(24, 12)))
            .show(root, |ui| self.header(ui));

        egui::CentralPanel::no_frame()
            .frame(egui::Frame::new().fill(BG).inner_margin(Margin::symmetric(24, 8)))
            .show(root, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| match self.page {
                        Page::Dashboard => self.dashboard(ui),
                        Page::Games => self.games_page(ui),
                        Page::Startup => self.startup_page(ui),
                        Page::Allege => self.allege_page(ui),
                        Page::Appearance => self.appearance_page(ui),
                        Page::Tools => self.tools_page(ui),
                    });
            });
    }

    fn nav(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("♦").size(30.0).color(ACCENT));
            ui.vertical(|ui| {
                ui.label(RichText::new("PRISM").size(20.0).strong().color(TEXT));
                ui.label(RichText::new("gaming · cybersécurité").small().color(MUTED));
            });
        });
        ui.add_space(26.0);
        for p in Page::ALL {
            let selected = self.page == p;
            let (fill, fg) = if selected {
                (ACCENT_DIM, ACCENT)
            } else {
                (Color32::TRANSPARENT, TEXT)
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
            if resp.clicked() {
                self.page = p;
            }
            ui.add_space(2.0);
        }
        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            ui.label(
                RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                    .small()
                    .color(MUTED),
            );
        });
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            ui.heading(self.page.label());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if !self.backend.elevated() && ui.button("Relancer en administrateur").clicked() {
                    let r = self.backend.relaunch_elevated();
                    self.result(r);
                }
                if self.live.watch_alive {
                    pill(ui, "✔ Prism actif", OK);
                } else {
                    if ui.button("Démarrer Prism").clicked() {
                        let r = self.backend.start_watch();
                        self.result(r);
                    }
                    pill(ui, "✖ Prism arrêté", BAD);
                }
                if let Some(e) = &self.live.etat {
                    if !e.game.is_empty() {
                        pill(ui, &format!("🎮 Mode Jeu : {}", e.game.join(", ")), ACCENT);
                    }
                }
                if let Some((msg, ok)) = &self.toast {
                    ui.label(RichText::new(msg).color(if *ok { OK } else { BAD }));
                }
            });
        });
    }

    // --- Tableau de bord ------------------------------------------------------

    fn dashboard(&mut self, ui: &mut egui::Ui) {
        section(ui, "Profil");
        let profiles = self.backend.profiles();
        let mut chosen = None;
        let current = self.live.profile.clone();
        row(ui, profiles.len().max(1), |i, col| {
            let p = &profiles[i];
            let active = p.name == current;
            let resp = card(col, active, |ui| {
                ui.label(
                    RichText::new(&p.label)
                        .size(17.0)
                        .strong()
                        .color(if active { ACCENT } else { TEXT }),
                );
                ui.add_space(2.0);
                ui.label(RichText::new(&p.description).small().color(MUTED));
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
                        .color(MUTED),
                    );
                    if ui.button("Libérer la RAM des applis inactives").clicked() {
                        clean = true;
                    }
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
                    ui.label(RichText::new("Inactives : cœurs économes, basse consommation, RAM rendue. Tout revient dès que vous y retournez.").small().color(MUTED));
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
                        ui.label(RichText::new("En attente").size(22.0).strong().color(MUTED));
                        ui.label(
                            RichText::new("S'active tout seul au lancement d'un jeu.")
                                .small()
                                .color(MUTED),
                        );
                    } else {
                        ui.label(RichText::new(game.join(", ")).size(22.0).strong().color(ACCENT));
                        ui.label(
                            RichText::new("Arrière-plan en retrait, RAM rendue, indexation en pause.")
                                .small()
                                .color(MUTED),
                        );
                    }
                    ui.add_space(4.0);
                    ui.label(RichText::new(&cores).small().color(MUTED));
                });
            }
        });
        if clean {
            let r = self.backend.ram_clean();
            self.result(r);
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
                                ui.label(RichText::new(h).small().color(MUTED));
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
                        ui.label(RichText::new("Rien pour l'instant.").color(MUTED));
                    }
                    for line in recent.iter().rev().take(10) {
                        ui.label(RichText::new(line).small());
                    }
                });
            }
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
                .color(MUTED),
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
            .frame(egui::Frame::new().fill(BG).inner_margin(Margin::symmetric(56, 40)))
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("♦ PRISM").size(28.0).strong().color(ACCENT));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new("Flèches : choisir · Entrée : jouer · Échap : quitter").color(MUTED));
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
                .color(MUTED),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Tout restaurer").clicked() {
                    action = Some(self.backend.startup_restore());
                }
                if ui
                    .add(egui::Button::new(RichText::new("Appliquer les conseils").color(BG)).fill(ACCENT))
                    .clicked()
                {
                    action = Some(self.backend.startup_recommended());
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
                            ui.label(RichText::new(r.source.label()).small().color(MUTED));
                        });
                        ui.label(RichText::new(&r.why).small().color(MUTED));
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

    fn allege_page(&mut self, ui: &mut egui::Ui) {
        let mut action: Option<Result<String, String>> = None;
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(
                    "72 services dont les anti-cheats et les mises à jour ont besoin ne sont jamais touchés.",
                )
                .color(MUTED),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Tout restaurer").clicked() {
                    action = Some(self.backend.allege_restore());
                }
            });
        });
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
        ] {
            ui.add_space(12.0);
            let rows: Vec<AllegeRow> = self.allege.iter().filter(|r| r.tier == tier).cloned().collect();
            let done = rows.iter().filter(|r| r.done).count();
            ui.horizontal(|ui| {
                section(ui, title);
                ui.label(
                    RichText::new(format!("{done}/{} appliqué(s) · {note}", rows.len()))
                        .small()
                        .color(MUTED),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button(format!("Appliquer « {title} »")).clicked() {
                        action = Some(self.backend.allege_apply(tier));
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
                            ui.label(RichText::new(if r.done { "✔" } else { "·" }).color(if r.done {
                                OK
                            } else {
                                MUTED
                            }));
                            ui.label(&r.label).on_hover_text(&r.why);
                            ui.label(
                                RichText::new(format!("{}  ›  {}", r.current, r.target))
                                    .small()
                                    .color(MUTED),
                            );
                            ui.end_row();
                        }
                    });
            });
        }
        if let Some(r) = action {
            self.result(r);
        }
    }

    // --- Apparence --------------------------------------------------------------

    fn appearance_page(&mut self, ui: &mut egui::Ui) {
        let mut action: Option<Result<String, String>> = None;
        ui.label(
            RichText::new("Réglages officiels de Windows et effets dessinés par Prism dans sa propre couche, tous réversibles. Rien n'est injecté dans les applis ni dans le compositeur de Windows : compatible avec les anti-cheats.")
                .color(MUTED),
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
                    ui.label(RichText::new(&p.description).small().color(MUTED));
                });
                if resp.interact(Sense::click()).clicked() {
                    action = Some(self.backend.appearance_preset(&p.id));
                }
            } else {
                let resp = card(col, false, |ui| {
                    ui.label(RichText::new("Réglages d'origine").size(17.0).strong());
                    ui.label(
                        RichText::new("Remet exactement ce que Windows avait avant Prism.")
                            .small()
                            .color(MUTED),
                    );
                });
                if resp.interact(Sense::click()).clicked() {
                    action = Some(self.backend.appearance_restore());
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
                                    let text = RichText::new(opt).color(if selected { BG } else { TEXT });
                                    let b = egui::Button::new(text).fill(if selected { ACCENT } else { CARD_HI });
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
        ui.horizontal(|ui| {
            section(ui, "Prism Bar");
            ui.label(
                RichText::new(
                    "Votre barre des tâches : n'importe quel bord, taille et opacité au choix, widgets système.",
                )
                .small()
                .color(MUTED),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if running {
                    if ui.button("Arrêter la barre").clicked() {
                        *action = Some(self.backend.bar_stop());
                    }
                    pill(ui, "✔ en marche", OK);
                } else if ui
                    .add(egui::Button::new(RichText::new("Lancer la barre").color(BG)).fill(ACCENT))
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
                        ui.label(RichText::new(name).small().color(MUTED));
                    };
                    field(ui, "Position");
                    ui.horizontal(|ui| {
                        for e in Edge::ALL {
                            let sel = cfg.edge == e;
                            let b = egui::Button::new(RichText::new(e.label()).color(if sel { BG } else { TEXT }))
                                .fill(if sel { ACCENT } else { CARD_HI });
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
                        ui.checkbox(&mut cfg.hide_in_fullscreen, "Se cacher en plein écran");
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
                    .color(MUTED),
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
        }
    }

    // --- Outils -----------------------------------------------------------------

    fn tools_page(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Rien n'est installé par défaut et rien ne tourne pendant le jeu. Les outils gênants pour les anti-cheats sont signalés.").color(MUTED));
        ui.add_space(8.0);
        let packs = self.backend.packs();
        let mut install = None;
        row(ui, 2, |c, col| {
            for p in packs.iter().skip(c).step_by(2) {
                card(col, false, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&p.label).size(17.0).strong());
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.button("Installer").clicked() {
                                install = Some(p.id.clone());
                            }
                        });
                    });
                    for (name, warn) in &p.tools {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("•").color(ACCENT));
                            ui.label(name);
                            if let Some(w) = warn {
                                ui.label(RichText::new("⚠ anti-cheat").small().color(WARN))
                                    .on_hover_text(w);
                            }
                        });
                    }
                });
                col.add_space(10.0);
            }
        });
        if let Some(p) = install {
            let r = self.backend.install_pack(&p);
            self.result(r);
        }
    }
}

impl eframe::App for PrismApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }
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
    ui.label(RichText::new(title.to_uppercase()).small().strong().color(MUTED));
    ui.add_space(2.0);
}

fn stat_title(ui: &mut egui::Ui, title: &str) {
    ui.label(RichText::new(title).small().color(ACCENT));
}

fn card(ui: &mut egui::Ui, highlighted: bool, add: impl FnOnce(&mut egui::Ui)) -> egui::Response {
    egui::Frame::new()
        .fill(if highlighted { ACCENT_DIM } else { CARD })
        .stroke(Stroke::new(1.0, if highlighted { ACCENT } else { BORDER }))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .response
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
        egui::Label::new(RichText::new(format!(" {text} ")).small().background_color(CARD_HI))
            .wrap_mode(egui::TextWrapMode::Extend),
    );
}

fn badge(ui: &mut egui::Ui, advice: &str) {
    let color = match advice {
        "à désactiver" => WARN,
        "protégé" => OK,
        "à garder" => OK,
        "optionnel" => ACCENT,
        _ => MUTED,
    };
    pill(ui, advice, color);
}

fn class_color(class: &str) -> Color32 {
    match class {
        "jeu" => ACCENT,
        "arrière-plan" => WARN,
        "compagnon" => OK,
        _ => MUTED,
    }
}

fn bar(ui: &mut egui::Ui, fraction: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 8.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(4), CARD_HI);
    let mut fill = rect;
    fill.set_width(rect.width() * fraction.clamp(0.0, 1.0));
    let color = if fraction > 0.85 {
        BAD
    } else if fraction > 0.7 {
        WARN
    } else {
        ACCENT
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
    p.rect_filled(rect, CornerRadius::same(12), if hovered { CARD_HI } else { CARD });
    p.rect_stroke(
        rect,
        CornerRadius::same(12),
        Stroke::new(if selected { 2.5 } else { 1.0 }, if hovered { ACCENT } else { BORDER }),
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
        TEXT,
    );
    p.text(
        rect.left_bottom() + Vec2::new(16.0, -14.0),
        egui::Align2::LEFT_BOTTOM,
        g.store.label(),
        egui::FontId::proportional(12.0),
        MUTED,
    );
    if hovered {
        p.text(
            rect.right_bottom() + Vec2::new(-16.0, -14.0),
            egui::Align2::RIGHT_BOTTOM,
            "▶ Jouer",
            egui::FontId::proportional(13.0),
            ACCENT,
        );
    }
    resp
}

/// Aperçu de la barre sur un écran miniature, avec le même code de disposition que
/// la vraie barre (`prism_core::bar::layout`).
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
    p.rect_filled(
        br,
        CornerRadius::same(radius),
        Color32::from_rgba_unmultiplied(0x12, 0x17, 0x1e, alpha),
    );
    for it in layout(cfg, bar.width(), bar.height(), 3) {
        let r = egui::Rect::from_min_max(
            br.min + Vec2::new(it.rect.left as f32 * k, it.rect.top as f32 * k),
            br.min + Vec2::new(it.rect.right as f32 * k, it.rect.bottom as f32 * k),
        )
        .shrink(1.5);
        let color = match it.widget {
            Widget::Start => ACCENT,
            Widget::Windows if it.index == Some(0) => ACCENT_DIM,
            Widget::Windows => CARD_HI,
            Widget::Cpu | Widget::Ram | Widget::Gpu => Color32::from_rgb(0x2e, 0x5a, 0x4c),
            Widget::GameMode => Color32::from_rgb(0x3a, 0x33, 0x60),
            Widget::Network => Color32::from_rgb(0x3d, 0x4a, 0x2a),
            Widget::Clock => Color32::from_rgb(0x44, 0x4c, 0x58),
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
        let bg = if *on { ACCENT } else { CARD_HI };
        let p = ui.painter();
        p.rect_filled(
            rect,
            CornerRadius::same(10),
            if enabled { bg } else { bg.gamma_multiply(0.4) },
        );
        let x = egui::lerp(rect.left() + 10.0..=rect.right() - 10.0, t);
        p.circle_filled(egui::pos2(x, rect.center().y), 7.0, if *on { BG } else { MUTED });
        resp
    }
}
