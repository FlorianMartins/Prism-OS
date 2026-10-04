//! Mises à jour : la dernière version publiée sur GitHub (Releases), son installateur
//! MSI et son empreinte SHA-256. Pure logique ; le téléchargement et l'installation
//! sont faits par `prism maj` sous Windows.
//!
//! Tant que les programmes ne sont pas signés (certificat de signature de code), la
//! confiance repose sur HTTPS vers GitHub et sur l'empreinte publiée avec la version :
//! un fichier qui ne correspond pas n'est jamais installé.

use serde::Deserialize;
use sha2::{Digest, Sha256};

pub const REPO: &str = "FlorianMartins/Prism-OS";

/// Adresse de la dernière version. `PRISM_UPDATE_URL` la remplace (tests, miroir).
pub fn latest_url() -> String {
    std::env::var("PRISM_UPDATE_URL").unwrap_or_else(|_| format!("https://api.github.com/repos/{REPO}/releases/latest"))
}

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// Une version plus récente, prête à être téléchargée.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Update {
    pub version: String,
    pub msi_name: String,
    pub msi_url: String,
    pub sums_url: String,
    /// Page de la version (notes).
    pub page: String,
}

/// `v0.7.0`, `0.7.0`, `0.7.0-beta` -> (0, 7, 0).
pub fn parse_version(s: &str) -> Option<(u32, u32, u32)> {
    let core = s.trim().trim_start_matches('v').split(['-', '+']).next()?;
    let mut it = core.split('.').map(|p| p.parse::<u32>().ok());
    let v = (
        it.next()??,
        it.next().unwrap_or(Some(0))?,
        it.next().unwrap_or(Some(0))?,
    );
    it.next().is_none().then_some(v)
}

/// Lit la réponse de GitHub. `Ok(None)` : déjà à jour.
pub fn from_release(json: &[u8], current: &str) -> Result<Option<Update>, String> {
    let r: Release = serde_json::from_slice(json).map_err(|e| format!("réponse de GitHub illisible : {e}"))?;
    if r.draft || r.prerelease {
        return Ok(None);
    }
    let (Some(latest), Some(mine)) = (parse_version(&r.tag_name), parse_version(current)) else {
        return Err(format!("numéro de version illisible : {} / {current}", r.tag_name));
    };
    if latest <= mine {
        return Ok(None);
    }
    let msi = r
        .assets
        .iter()
        .find(|a| a.name.starts_with("prism-") && a.name.ends_with("-x64.msi"))
        .ok_or("la version publiée n'a pas d'installateur MSI")?;
    let sums = r.assets.iter().find(|a| a.name == "SHA256SUMS").ok_or(
        "la version publiée n'a pas de fichier d'empreintes (SHA256SUMS) : pas d'installation sans vérification",
    )?;
    Ok(Some(Update {
        version: r.tag_name.trim_start_matches('v').to_string(),
        msi_name: msi.name.clone(),
        msi_url: msi.browser_download_url.clone(),
        sums_url: sums.browser_download_url.clone(),
        page: r.html_url,
    }))
}

/// Empreinte attendue d'un fichier dans un `SHA256SUMS` (format de `sha256sum`).
pub fn expected_sha256(sums: &str, file: &str) -> Option<[u8; 32]> {
    sums.lines().find_map(|line| {
        let (hash, name) = line.trim().split_once(char::is_whitespace)?;
        let name = name.trim().trim_start_matches('*');
        (name == file).then(|| parse_hex32(hash)).flatten()
    })
}

fn parse_hex32(s: &str) -> Option<[u8; 32]> {
    let s = s.trim();
    if s.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(s.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}

pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

pub fn hex(h: &[u8; 32]) -> String {
    h.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, with_sums: bool) -> String {
        let sums = if with_sums {
            r#",{"name":"SHA256SUMS","browser_download_url":"https://x/SHA256SUMS"}"#
        } else {
            ""
        };
        format!(
            r#"{{"tag_name":"{tag}","html_url":"https://github.com/r/{tag}","assets":[{{"name":"prism-0.7.0-x64.msi","browser_download_url":"https://x/prism.msi"}}{sums}]}}"#
        )
    }

    #[test]
    fn versions_are_compared_numerically() {
        assert_eq!(parse_version("v0.10.2"), Some((0, 10, 2)));
        assert_eq!(parse_version("1.2"), Some((1, 2, 0)));
        assert_eq!(parse_version("0.7.0-beta.1"), Some((0, 7, 0)));
        assert_eq!(parse_version("abc"), None);
        assert!(parse_version("0.10.0") > parse_version("0.9.9"), "pas un tri de texte");
    }

    #[test]
    fn a_newer_release_is_offered_only_with_its_checksums() {
        let u = from_release(release("v0.7.0", true).as_bytes(), "0.1.0")
            .unwrap()
            .unwrap();
        assert_eq!(u.version, "0.7.0");
        assert_eq!(u.msi_url, "https://x/prism.msi");
        assert!(
            from_release(release("v0.7.0", true).as_bytes(), "0.7.0")
                .unwrap()
                .is_none(),
            "à jour"
        );
        assert!(
            from_release(release("v0.6.0", true).as_bytes(), "0.7.0")
                .unwrap()
                .is_none(),
            "jamais en arrière"
        );
        assert!(
            from_release(release("v0.7.0", false).as_bytes(), "0.1.0").is_err(),
            "pas d'empreintes, pas d'installation"
        );
        let pre = release("v0.8.0", true).replace(r#""assets""#, r#""prerelease":true,"assets""#);
        assert!(
            from_release(pre.as_bytes(), "0.1.0").unwrap().is_none(),
            "pré-version ignorée"
        );
    }

    #[test]
    fn checksums_are_read_like_sha256sum_and_verified() {
        let data = b"prism";
        let h = sha256(data);
        let sums = format!("{}  prism-0.7.0-x64.msi\n{}  autre.txt\n", hex(&h), "00".repeat(32));
        assert_eq!(expected_sha256(&sums, "prism-0.7.0-x64.msi"), Some(h));
        assert_eq!(
            expected_sha256(&format!("{} *prism.msi", hex(&h)), "prism.msi"),
            Some(h),
            "mode binaire"
        );
        assert_eq!(expected_sha256(&sums, "absent.msi"), None);
        assert_eq!(expected_sha256("zz  f", "f"), None);
        // Valeur de référence : SHA-256 de « abc ».
        assert_eq!(
            hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
