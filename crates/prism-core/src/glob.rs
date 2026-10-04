//! Motifs de noms minimalistes : `*` remplace n'importe quelle suite de caractères.
//! Pas d'autre métacaractère, pour que la configuration reste lisible.

/// `pattern` et `text` sont comparés tels quels (la config est en minuscules et les
/// noms de processus sont mis en minuscules au relevé).
pub fn matches(pattern: &str, text: &str) -> bool {
    let p = pattern.as_bytes();
    let t = text.as_bytes();
    let (mut pi, mut ti) = (0usize, 0usize);
    // Position du dernier `*` vu, et du texte quand on l'a vu (retour arrière borné).
    let mut star: Option<(usize, usize)> = None;
    while ti < t.len() {
        if pi < p.len() && p[pi] == b'*' {
            star = Some((pi, ti));
            pi += 1;
        } else if pi < p.len() && p[pi] == t[ti] {
            pi += 1;
            ti += 1;
        } else if let Some((sp, st)) = star {
            pi = sp + 1;
            ti = st + 1;
            star = Some((sp, st + 1));
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }
    pi == p.len()
}

pub fn any_matches<'a, I: IntoIterator<Item = &'a String>>(patterns: I, text: &str) -> bool {
    patterns.into_iter().any(|p| matches(p, text))
}

#[cfg(test)]
mod tests {
    use super::matches;

    #[test]
    fn literal() {
        assert!(matches("vgc.exe", "vgc.exe"));
        assert!(!matches("vgc.exe", "vgc.exe2"));
        assert!(!matches("vgc.exe", "xvgc.exe"));
    }

    #[test]
    fn wildcards() {
        assert!(matches("easyanticheat*.exe", "easyanticheat_eos.exe"));
        assert!(matches("easyanticheat*.exe", "easyanticheat.exe"));
        assert!(!matches("easyanticheat*.exe", "easyanticheat.dll"));
        assert!(matches("*", ""));
        assert!(matches("ace-*.exe", "ace-tray.exe"));
        assert!(matches("*dbg*", "x64dbg.exe"));
        assert!(!matches("a*b*c", "acb"));
        assert!(matches("a*b*c", "axxbyyc"));
    }

    #[test]
    fn backtracking_is_correct() {
        assert!(matches("*.exe", "a.exe.exe"));
        assert!(matches("*aab", "aaaab"));
        assert!(!matches("*aab", "aaaa"));
    }
}
