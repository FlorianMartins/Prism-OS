//! Catalogue d'outils cyber : commandes d'installation et détection des conflits.
//!
//! Les commandes sont des listes d'arguments, jamais une ligne passée à un shell. Les
//! noms de paquets viennent d'un fichier que l'utilisateur peut modifier : ils sont
//! validés pour qu'aucun ne puisse se faire passer pour une option (`-o …`).

use crate::classify::{classify_all, Class};
use crate::config::{Config, Tool, ToolSource};
use crate::model::Snapshot;

pub const KALI_DISTRO: &str = "kali-linux";

fn valid_token(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with('-')
        && s.len() <= 128
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+'))
}

pub fn validate_package(t: &Tool) -> Result<(), String> {
    let tokens: Vec<&str> = match t.source {
        ToolSource::KaliApt => t.package.split_whitespace().collect(),
        _ => vec![t.package.as_str()],
    };
    if tokens.is_empty() || !tokens.iter().all(|tok| valid_token(tok)) {
        return Err(format!("outil {} : nom de paquet refusé « {} »", t.id, t.package));
    }
    Ok(())
}

/// Commandes (argv) qui installent l'outil.
pub fn install_commands(t: &Tool) -> Vec<Vec<String>> {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<String>>();
    match t.source {
        ToolSource::Winget => vec![s(&[
            "winget",
            "install",
            "--id",
            &t.package,
            "--exact",
            "--source",
            "winget",
            "--accept-package-agreements",
            "--accept-source-agreements",
            "--silent",
            "--disable-interactivity",
        ])],
        ToolSource::WslDistro => vec![s(&["wsl", "--install", "-d", &t.package, "--no-launch"])],
        ToolSource::External => Vec::new(),
        ToolSource::KaliApt => {
            let mut install = s(&["wsl", "-d", KALI_DISTRO, "-u", "root", "--", "apt-get", "install", "-y"]);
            install.extend(t.package.split_whitespace().map(String::from));
            vec![
                s(&["wsl", "-d", KALI_DISTRO, "-u", "root", "--", "apt-get", "update"]),
                install,
            ]
        }
    }
}

/// Commandes (argv) qui désinstallent l'outil. Pour Kali (WSL), `--unregister` efface
/// la distribution et tous ses fichiers : l'interface demande confirmation.
pub fn uninstall_commands(t: &Tool) -> Vec<Vec<String>> {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<String>>();
    match t.source {
        ToolSource::Winget => vec![s(&[
            "winget",
            "uninstall",
            "--id",
            &t.package,
            "--exact",
            "--silent",
            "--accept-source-agreements",
            "--disable-interactivity",
        ])],
        ToolSource::WslDistro => vec![s(&["wsl", "--unregister", &t.package])],
        ToolSource::KaliApt => {
            let mut remove = s(&["wsl", "-d", KALI_DISTRO, "-u", "root", "--", "apt-get", "remove", "-y"]);
            remove.extend(t.package.split_whitespace().map(String::from));
            vec![remove]
        }
        ToolSource::External => Vec::new(),
    }
}

/// Efface-t-on des données de l'utilisateur en désinstallant ?
pub fn uninstall_erases_data(t: &Tool) -> bool {
    t.source == ToolSource::WslDistro
}

/// Identifiants winget présents dans la sortie de `winget list` (colonne « Id »,
/// insensible à la casse ; la sortie est un tableau aligné, localisé).
pub fn winget_installed(list_output: &str, ids: &[&str]) -> Vec<String> {
    let tokens: std::collections::HashSet<String> = list_output
        .split_whitespace()
        .map(|t| t.trim_matches(|c: char| c == '…').to_ascii_lowercase())
        .collect();
    ids.iter()
        .filter(|id| tokens.contains(&id.to_ascii_lowercase()))
        .map(|id| id.to_string())
        .collect()
}

/// Distributions WSL dans la sortie de `wsl -l -q` (UTF-16 sur Windows : à décoder
/// avant). Les octets nuls résiduels sont ignorés.
pub fn wsl_distros(list_output: &str) -> Vec<String> {
    list_output
        .lines()
        .map(|l| l.replace('\0', "").trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// Outils d'un pack, dépendances comprises et placées avant ceux qui les requièrent.
pub fn resolve_pack(cfg: &Config, pack: &str) -> Result<Vec<Tool>, String> {
    let p = cfg.packs.get(pack).ok_or_else(|| {
        let known: Vec<&str> = cfg.packs.keys().map(String::as_str).collect();
        format!("pack inconnu « {pack} » (connus : {})", known.join(", "))
    })?;
    let mut out: Vec<Tool> = Vec::new();
    for id in &p.tools {
        push_with_requirements(cfg, id, &mut out, 0)?;
    }
    Ok(out)
}

/// Outils demandés, dépendances comprises et placées avant (installer un outil Kali
/// installe Kali d'abord).
pub fn resolve_tools(cfg: &Config, ids: &[String]) -> Result<Vec<Tool>, String> {
    let mut out: Vec<Tool> = Vec::new();
    for id in ids {
        push_with_requirements(cfg, id, &mut out, 0)?;
    }
    Ok(out)
}

fn push_with_requirements(cfg: &Config, id: &str, out: &mut Vec<Tool>, depth: u32) -> Result<(), String> {
    if depth > 8 {
        return Err(format!("dépendances circulaires autour de « {id} »"));
    }
    if out.iter().any(|t| t.id == id) {
        return Ok(());
    }
    let tool = cfg.tool(id).ok_or_else(|| format!("outil inconnu « {id} »"))?;
    for r in &tool.requires {
        push_with_requirements(cfg, r, out, depth + 1)?;
    }
    out.push(tool.clone());
    Ok(())
}

/// Outil gênant pour les anti-cheats, ouvert en ce moment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflict {
    pub tool: String,
    pub process: String,
    pub reason: String,
}

pub fn conflicts(snap: &Snapshot, cfg: &Config) -> Vec<Conflict> {
    let mut out: Vec<Conflict> = classify_all(snap, cfg)
        .into_iter()
        .filter_map(|(p, c)| match c {
            Class::Conflict { tool } => {
                let reason = cfg.tool(&tool).and_then(|t| t.reason.clone()).unwrap_or_default();
                Some(Conflict {
                    tool,
                    process: p.name.clone(),
                    reason,
                })
            }
            _ => None,
        })
        .collect();
    out.sort_by(|a, b| a.process.cmp(&b.process));
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classify::tests::proc;

    #[test]
    fn winget_command_is_exact_and_non_interactive() {
        let cfg = Config::builtin();
        let cmd = &install_commands(cfg.tool("wireshark").unwrap())[0];
        assert_eq!(
            &cmd[..4],
            ["winget", "install", "--id", "WiresharkFoundation.Wireshark"]
        );
        assert!(cmd.contains(&"--exact".to_string()));
    }

    #[test]
    fn uninstall_is_silent_and_erasing_kali_is_flagged() {
        let cfg = Config::builtin();
        let w = &uninstall_commands(cfg.tool("wireshark").unwrap())[0];
        assert_eq!(
            &w[..4],
            ["winget", "uninstall", "--id", "WiresharkFoundation.Wireshark"]
        );
        assert!(w.contains(&"--silent".to_string()));
        let kali = cfg.tool("kali").unwrap();
        assert_eq!(uninstall_commands(kali)[0], ["wsl", "--unregister", "kali-linux"]);
        assert!(uninstall_erases_data(kali));
        assert!(!uninstall_erases_data(cfg.tool("x64dbg").unwrap()));
        assert!(uninstall_commands(cfg.tool("cheatengine").unwrap()).is_empty());
    }

    #[test]
    fn installed_tools_are_read_from_winget_and_wsl_lists() {
        let list = "Name                 Id                              Version   Source\n\
                    ---------------------------------------------------------------\n\
                    Wireshark 4.4.1      WiresharkFoundation.Wireshark   4.4.1     winget\n\
                    PuTTY release 0.81   PuTTY.PuTTY                     0.81.0.0  winget\n";
        let found = winget_installed(list, &["WiresharkFoundation.Wireshark", "x64dbg.x64dbg", "putty.putty"]);
        assert_eq!(found, ["WiresharkFoundation.Wireshark", "putty.putty"]);
        assert_eq!(wsl_distros("Ubuntu\r\nkali-linux\r\n\r\n"), ["Ubuntu", "kali-linux"]);
    }

    #[test]
    fn kali_pack_installs_the_distro_first() {
        let cfg = Config::builtin();
        let tools = resolve_pack(&cfg, "kali").unwrap();
        let ids: Vec<&str> = tools.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["kali", "kali-essentiels"]);
        let cmds = install_commands(&tools[1]);
        assert!(cmds[0].ends_with(&["apt-get".into(), "update".into()]));
        assert!(cmds[1].contains(&"nmap".to_string()) && cmds[1].contains(&"hashcat".to_string()));
    }

    #[test]
    fn every_pack_of_every_profile_resolves() {
        let cfg = Config::builtin();
        for (name, p) in &cfg.profiles {
            for pack in &p.tool_packs {
                assert!(resolve_pack(&cfg, pack).is_ok(), "profil {name}, pack {pack}");
            }
        }
    }

    #[test]
    fn package_names_cannot_smuggle_options() {
        let mut t = Config::builtin().tool("kali-essentiels").unwrap().clone();
        t.package = "nmap -o APT::Get::AllowUnauthenticated=true".into();
        assert!(validate_package(&t).is_err());
        t.package = "nmap;rm".into();
        assert!(validate_package(&t).is_err());
        let mut w = Config::builtin().tool("burp").unwrap().clone();
        w.package = "--override".into();
        assert!(validate_package(&w).is_err());
    }

    #[test]
    fn open_debugger_is_reported_with_its_reason() {
        let cfg = Config::builtin();
        let snap = Snapshot {
            procs: vec![proc(10, "x64dbg.exe", None), proc(11, "wireshark.exe", None)],
            user_session: 1,
            self_pid: 999,
            ..Default::default()
        };
        let c = conflicts(&snap, &cfg);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].tool, "x64dbg");
        assert!(c[0].reason.contains("anti-cheat"));
    }
}
