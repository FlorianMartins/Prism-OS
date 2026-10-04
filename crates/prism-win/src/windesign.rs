//! Design de Windows calé sur le thème de Prism : couleur d'accent (barres de titre,
//! bordures, sélections, menu Démarrer de Windows) et fond d'écran généré. Les valeurs
//! d'origine sont notées avant tout changement : `restore` les remet.

use std::ptr::null_mut;

use prism_core::theme::Palette;
use prism_core::windesign::{abgr, accent_palette, argb, start_color, AccentAvant, Origine};
use windows_sys::Win32::System::Registry::{
    RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_BINARY, REG_DWORD, RRF_RT_REG_BINARY,
    RRF_RT_REG_DWORD,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    SendMessageTimeoutW, SystemParametersInfoW, HWND_BROADCAST, SMTO_ABORTIFHUNG, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE,
    SPI_GETDESKWALLPAPER, SPI_SETDESKWALLPAPER, WM_SETTINGCHANGE,
};

const DWM: &str = "Software\\Microsoft\\Windows\\DWM";
const ACCENT: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Accent";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn get_dword(key: &str, value: &str) -> Option<u32> {
    let (k, v) = (wide(key), wide(value));
    let mut data = 0u32;
    let mut len = 4u32;
    // SAFETY: tampon local de 4 octets annoncés.
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            k.as_ptr(),
            v.as_ptr(),
            RRF_RT_REG_DWORD,
            null_mut(),
            &mut data as *mut u32 as *mut _,
            &mut len,
        )
    };
    (r == 0).then_some(data)
}

fn get_binary(key: &str, value: &str) -> Option<Vec<u8>> {
    let (k, v) = (wide(key), wide(value));
    let mut buf = vec![0u8; 256];
    let mut len = buf.len() as u32;
    // SAFETY: tampon local de taille annoncée.
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            k.as_ptr(),
            v.as_ptr(),
            RRF_RT_REG_BINARY,
            null_mut(),
            buf.as_mut_ptr() as *mut _,
            &mut len,
        )
    };
    (r == 0).then(|| {
        buf.truncate(len as usize);
        buf
    })
}

fn set_dword(key: &str, value: &str, data: u32) -> Result<(), String> {
    let (k, v) = (wide(key), wide(value));
    // SAFETY: données locales de 4 octets.
    let r = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            k.as_ptr(),
            v.as_ptr(),
            REG_DWORD,
            &data as *const u32 as *const _,
            4,
        )
    };
    if r == 0 {
        Ok(())
    } else {
        Err(format!("{key}\\{value} : erreur {r}"))
    }
}

fn set_binary(key: &str, value: &str, data: &[u8]) -> Result<(), String> {
    let (k, v) = (wide(key), wide(value));
    // SAFETY: tranche locale de taille annoncée.
    let r = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            k.as_ptr(),
            v.as_ptr(),
            REG_BINARY,
            data.as_ptr() as *const _,
            data.len() as u32,
        )
    };
    if r == 0 {
        Ok(())
    } else {
        Err(format!("{key}\\{value} : erreur {r}"))
    }
}

fn put_dword(key: &str, value: &str, data: Option<u32>) {
    match data {
        Some(d) => {
            let _ = set_dword(key, value, d);
        }
        None => {
            let (k, v) = (wide(key), wide(value));
            // SAFETY: chaînes locales.
            unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, k.as_ptr(), v.as_ptr()) };
        }
    }
}

fn broadcast(what: &str) {
    let w = wide(what);
    let mut res = 0usize;
    // SAFETY: message de diffusion standard, délai borné.
    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            w.as_ptr() as isize,
            SMTO_ABORTIFHUNG,
            2000,
            &mut res,
        );
    }
}

fn wallpaper_now() -> Option<String> {
    let mut buf = [0u16; 520];
    // SAFETY: tampon local de taille annoncée (en caractères).
    let ok =
        unsafe { SystemParametersInfoW(SPI_GETDESKWALLPAPER, buf.len() as u32, buf.as_mut_ptr() as *mut _, 0) } != 0;
    let n = buf.iter().position(|c| *c == 0).unwrap_or(0);
    (ok && n > 0).then(|| String::from_utf16_lossy(&buf[..n]))
}

fn set_wallpaper(path: &str) -> Result<(), String> {
    let w = wide(path);
    // SAFETY: chaîne locale terminée par zéro.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_SETDESKWALLPAPER,
            0,
            w.as_ptr() as *mut _,
            SPIF_UPDATEINIFILE | SPIF_SENDCHANGE,
        )
    } != 0;
    if ok {
        Ok(())
    } else {
        Err("fond d'écran refusé par Windows".into())
    }
}

/// Applique l'accent du thème à Windows et/ou un fond d'écran généré à la taille de
/// l'écran principal. Rend la liste de ce qui a été fait.
pub fn apply(p: &Palette, accent: bool, fond: bool, dir: &std::path::Path) -> Result<Vec<String>, String> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
    // SAFETY: lectures sans paramètre.
    let size = unsafe {
        (
            GetSystemMetrics(SM_CXSCREEN) as usize,
            GetSystemMetrics(SM_CYSCREEN) as usize,
        )
    };
    let mut origine = Origine::charger(dir);
    let mut done = Vec::new();
    if accent {
        if origine.accent.is_none() {
            origine.accent = Some(AccentAvant {
                accent_color: get_dword(DWM, "AccentColor"),
                colorization_color: get_dword(DWM, "ColorizationColor"),
                colorization_afterglow: get_dword(DWM, "ColorizationAfterglow"),
                accent_color_menu: get_dword(ACCENT, "AccentColorMenu"),
                start_color_menu: get_dword(ACCENT, "StartColorMenu"),
                accent_palette: get_binary(ACCENT, "AccentPalette"),
            });
            origine.enregistrer(dir)?;
        }
        let a = p.accent;
        set_binary(ACCENT, "AccentPalette", &accent_palette(a))?;
        set_dword(ACCENT, "AccentColorMenu", abgr(a))?;
        set_dword(ACCENT, "StartColorMenu", abgr(start_color(a)))?;
        set_dword(DWM, "AccentColor", abgr(a))?;
        set_dword(DWM, "ColorizationColor", argb(a))?;
        set_dword(DWM, "ColorizationAfterglow", argb(a))?;
        broadcast("ImmersiveColorSet");
        done.push("couleur d'accent de Windows".to_string());
    }
    if fond {
        if origine.fond.is_none() {
            origine.fond = Some(wallpaper_now().unwrap_or_default());
            origine.enregistrer(dir)?;
        }
        let (w, h) = (size.0.clamp(640, 3840), size.1.clamp(360, 2160));
        let img = prism_core::wallpaper::render(w, h, p);
        let path = dir.join("fond-prism.bmp");
        std::fs::write(&path, prism_core::wallpaper::bmp(w, h, &img)).map_err(|e| e.to_string())?;
        set_wallpaper(&path.display().to_string())?;
        done.push("fond d'écran".to_string());
    }
    Ok(done)
}

/// Remet l'accent et le fond d'écran d'avant Prism.
pub fn restore(dir: &std::path::Path) -> Result<Vec<String>, String> {
    let origine = Origine::charger(dir);
    let mut done = Vec::new();
    if let Some(a) = &origine.accent {
        put_dword(DWM, "AccentColor", a.accent_color);
        put_dword(DWM, "ColorizationColor", a.colorization_color);
        put_dword(DWM, "ColorizationAfterglow", a.colorization_afterglow);
        put_dword(ACCENT, "AccentColorMenu", a.accent_color_menu);
        put_dword(ACCENT, "StartColorMenu", a.start_color_menu);
        if let Some(pal) = &a.accent_palette {
            let _ = set_binary(ACCENT, "AccentPalette", pal);
        }
        broadcast("ImmersiveColorSet");
        done.push("couleur d'accent de Windows".to_string());
    }
    if let Some(f) = &origine.fond {
        let _ = set_wallpaper(f);
        done.push("fond d'écran".to_string());
    }
    Origine::default().enregistrer(dir)?;
    Ok(done)
}
