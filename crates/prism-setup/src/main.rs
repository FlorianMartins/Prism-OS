// Pas de fenêtre console derrière l'installateur sous Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    #[cfg(windows)]
    {
        if !prism_win::is_elevated() {
            // Installation pour la machine : droits administrateur demandés tout de suite,
            // comme tout installateur (refus : rien n'est fait).
            let _ = prism_win::relaunch_elevated();
            return;
        }
    }
    let installer = new_installer();
    let run = |renderer: eframe::Renderer, app: prism_setup::SetupApp| {
        let options = eframe::NativeOptions {
            renderer,
            viewport: eframe::egui::ViewportBuilder::default()
                .with_title("Installation de Prism OS")
                .with_inner_size([760.0, 560.0])
                .with_resizable(false),
            ..Default::default()
        };
        eframe::run_native(
            "Installation de Prism OS",
            options,
            Box::new(move |_| Ok(Box::new(Window(app)))),
        )
    };
    let auto = {
        let args: Vec<String> = std::env::args().collect();
        args.iter()
            .position(|a| a == "--auto")
            .and_then(|i| args.get(i + 1).cloned())
    };
    let make = |installer| {
        let mut a = prism_setup::SetupApp::new(installer);
        a.auto = auto.clone();
        a
    };
    if run(eframe::Renderer::Wgpu, make(installer)).is_err() {
        // DirectX 12 / Vulkan indisponibles : OpenGL.
        let _ = run(eframe::Renderer::Glow, make(new_installer()));
    }
}

struct Window(prism_setup::SetupApp);

impl eframe::App for Window {
    fn ui(&mut self, ui: &mut eframe::egui::Ui, _frame: &mut eframe::Frame) {
        self.0.show(ui);
    }
}

fn new_installer() -> Box<dyn prism_setup::Installer> {
    #[cfg(windows)]
    return Box::new(MsiInstaller);
    #[cfg(not(windows))]
    return Box::new(prism_setup::MockInstaller::default());
}

#[cfg(windows)]
static MSI: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/prism.msi"));

/// Installation réelle : le paquet MSI embarqué, installé par Windows Installer.
#[cfg(windows)]
struct MsiInstaller;

#[cfg(windows)]
impl prism_setup::Installer for MsiInstaller {
    fn default_folder(&self) -> String {
        prism_win::setup::default_folder()
    }

    fn installed_version(&self) -> Option<String> {
        prism_win::setup::installed_version()
    }

    fn pick_folder(&self, current: &str) -> Option<String> {
        prism_win::setup::pick_folder("Dossier d'installation de Prism")
            .map(|f| {
                // Un dossier choisi comme « D:\Programmes » reçoit son sous-dossier Prism.
                if f.to_ascii_lowercase().ends_with("prism") {
                    f
                } else {
                    format!("{}\\Prism", f.trim_end_matches('\\'))
                }
            })
            .or_else(|| Some(current.to_string()))
    }

    fn start(&mut self, folder: &str, progress: prism_setup::Shared) {
        let folder = folder.to_string();
        std::thread::spawn(move || {
            let set = |f: &dyn Fn(&mut prism_setup::Progress)| {
                if let Ok(mut p) = progress.lock() {
                    f(&mut p);
                }
            };
            if MSI.is_empty() {
                set(&|p| {
                    p.done = Some(Err(
                        "installateur absent de ce programme (construction incomplète)".into()
                    ))
                });
                return;
            }
            let dir = std::env::temp_dir().join("prism-setup");
            let msi = dir.join("prism.msi");
            let log = dir.join("installation.log");
            if let Err(e) = std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&msi, MSI)) {
                set(&|p| p.done = Some(Err(format!("fichier temporaire : {e}"))));
                return;
            }
            let r = prism_win::setup::install(&msi, &folder, &log, &mut |e| match e {
                prism_win::setup::Event::Fraction(f) => set(&|p| p.fraction = f),
                prism_win::setup::Event::Action(a) => {
                    let step = match a {
                        "InstallValidate" | "RemoveExistingProducts" => 0,
                        "InstallFiles" => 1,
                        "PrismAfterInstall" | "CreateShortcuts" | "WriteRegistryValues" => 2,
                        _ => 3,
                    };
                    set(&|p| p.step = p.step.max(step));
                }
            });
            let _ = std::fs::remove_file(&msi);
            set(&|p| {
                if r.is_ok() {
                    p.fraction = 1.0;
                    p.step = prism_setup::STEPS.len();
                }
                p.done = Some(r.clone());
            });
        });
    }

    fn launch(&self, folder: &str) {
        prism_win::setup::launch_unelevated(&std::path::Path::new(folder).join("prism-ui.exe"));
    }
}
