//! Embarque l'installateur MSI (chemin dans PRISM_MSI, fourni par la CI après sa
//! construction). Sans lui, un fichier vide : le programme le signale à l'écran.
fn main() {
    windows_resources("Installation de Prism OS", &["prism-setup"]);
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("prism.msi");
    println!("cargo:rerun-if-env-changed=PRISM_MSI");
    match std::env::var("PRISM_MSI") {
        Ok(p) if !p.is_empty() => {
            println!("cargo:rerun-if-changed={p}");
            std::fs::copy(&p, &out).expect("PRISM_MSI introuvable");
        }
        _ => std::fs::write(&out, b"").unwrap(),
    }
}

/// Icône et informations du fichier (Propriétés › Détails, fenêtre de téléchargement),
/// pour les seuls exécutables listés (pas pour les crates qui utilisent la bibliothèque).
fn windows_resources(description: &str, bins: &[&str]) {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let v = env!("CARGO_PKG_VERSION");
    let parts: Vec<&str> = v.split('.').collect();
    let num = format!(
        "{},{},{},0",
        parts[0],
        parts.get(1).unwrap_or(&"0"),
        parts.get(2).unwrap_or(&"0")
    );
    let icon = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../installer/prism.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    let rc = format!(
        r#"#pragma code_page(65001)
1 ICON "{icon}"
1 VERSIONINFO
FILEVERSION {num}
PRODUCTVERSION {num}
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040C04B0"
    BEGIN
      VALUE "CompanyName", "Hivey"
      VALUE "ProductName", "Prism OS"
      VALUE "FileDescription", "{description}"
      VALUE "FileVersion", "{v}"
      VALUE "ProductVersion", "{v}"
      VALUE "LegalCopyright", "© 2026 Hivey — licence MIT"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x040C, 1200
  END
END
"#,
        icon = icon.display().to_string().replace('\\', "/"),
    );
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("prism.rc");
    std::fs::write(&out, rc).unwrap();
    embed_resource::compile_for(&out, bins, embed_resource::NONE)
        .manifest_optional()
        .unwrap_or_else(|e| println!("cargo:warning=ressources Windows non intégrées : {e}"));
}
