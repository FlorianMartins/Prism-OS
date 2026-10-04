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
        ])],
        ToolSource::WslDistro => vec![s(&["wsl", "--install", "-d", &t.package])],
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
