//! Démarrage automatique de Prism avec Windows (définitions des tâches planifiées).
//!
//! - Moteur (`prism watch`) : au **démarrage de Windows**, sous le compte système, donc
//!   actif avant même l'écran de connexion ; sans limite de durée (une tâche planifiée
//!   s'arrête d'office au bout de 3 jours), en priorité normale (sinon « inférieure à la
//!   normale », héritée par ce qu'il lance), même sur batterie, relancé s'il s'arrête.
//! - Prism Bar : à l'**ouverture de session** de tout utilisateur, avec ses droits
//!   (administrateur si le compte l'est).

pub const ENGINE_TASK: &str = "Prism OS";
pub const BAR_TASK: &str = "Prism Bar";

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

const SETTINGS: &str = "  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>true</StartWhenAvailable>
    <IdleSettings><StopOnIdleEnd>false</StopOnIdleEnd><RestartOnIdle>false</RestartOnIdle></IdleSettings>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>4</Priority>
    <RestartOnFailure><Interval>PT1M</Interval><Count>5</Count></RestartOnFailure>
  </Settings>
";

/// Définition XML d'une tâche (format du Planificateur, version 1.2).
fn task(description: &str, trigger: &str, principal: &str, exe: &str, args: &str, dir: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-16\"?>
<Task version=\"1.2\" xmlns=\"http://schemas.microsoft.com/windows/2004/02/mit/task\">
  <RegistrationInfo><Author>Hivey</Author><Description>{}</Description></RegistrationInfo>
  <Triggers>{trigger}</Triggers>
  <Principals><Principal id=\"Author\">{principal}</Principal></Principals>
{SETTINGS}  <Actions Context=\"Author\"><Exec><Command>{}</Command><Arguments>{}</Arguments><WorkingDirectory>{}</WorkingDirectory></Exec></Actions>
</Task>
",
        escape(description),
        escape(exe),
        escape(args),
        escape(dir)
    )
}

/// Moteur : au démarrage de Windows, compte système (`S-1-5-18`).
pub fn engine_task_xml(dir: &str) -> String {
    task(
        "Prism OS : moteur (Mode Quotidien, Mode Jeu, nettoyage de la RAM), actif dès le démarrage de Windows.",
        "<BootTrigger><Enabled>true</Enabled></BootTrigger>",
        "<UserId>S-1-5-18</UserId><RunLevel>HighestAvailable</RunLevel>",
        &format!("{dir}\\prism.exe"),
        "watch --quiet",
        dir,
    )
}

/// Barre : à l'ouverture de session de tout utilisateur (groupe Utilisateurs,
/// `S-1-5-32-545`), avec les droits les plus élevés du compte.
pub fn bar_task_xml(dir: &str) -> String {
    task(
        "Prism OS : Prism Bar, à chaque ouverture de session.",
        "<LogonTrigger><Enabled>true</Enabled></LogonTrigger>",
        "<GroupId>S-1-5-32-545</GroupId><RunLevel>HighestAvailable</RunLevel>",
        &format!("{dir}\\prism-bar.exe"),
        "",
        dir,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_starts_at_boot_as_system_without_time_limit() {
        let x = engine_task_xml(r"C:\Program Files\Prism");
        for needle in [
            "<BootTrigger>",
            "<UserId>S-1-5-18</UserId>",
            "<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>",
            "<Priority>4</Priority>",
            "<DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>",
            "<RestartOnFailure>",
            r"<Command>C:\Program Files\Prism\prism.exe</Command>",
            "<Arguments>watch --quiet</Arguments>",
        ] {
            assert!(x.contains(needle), "{needle} absent");
        }
    }

    #[test]
    fn bar_starts_at_any_logon_and_paths_are_escaped() {
        let x = bar_task_xml(r"C:\Jeux & Outils\Prism");
        assert!(x.contains("<LogonTrigger>") && x.contains("<GroupId>S-1-5-32-545</GroupId>"));
        assert!(x.contains(r"C:\Jeux &amp; Outils\Prism\prism-bar.exe"));
        assert!(!x.contains("& Outils"));
    }
}
