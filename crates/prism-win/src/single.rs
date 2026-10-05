//! Une seule fenêtre Prism : relancer l'appli (menu Démarrer, icône, raccourci) ramène
//! celle déjà ouverte au lieu d'en ouvrir une autre.
//!
//! La fenêtre ouverte crée un événement nommé et l'attend ; un nouveau lancement le
//! signale puis se ferme. L'appli tourne souvent avec les droits administrateur (relancée
//! par la tâche « Prism (admin) ») alors qu'un nouveau lancement n'en a pas : l'événement
//! porte une étiquette d'intégrité basse (`S:(ML;;NW;;;LW)`), sans quoi Windows refuse
//! qu'un processus ordinaire y écrive.

use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, LocalFree, ERROR_ALREADY_EXISTS, HWND, LPARAM};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::System::Threading::{
    CreateEventW, GetCurrentProcessId, OpenEventW, SetEvent, WaitForSingleObject, EVENT_MODIFY_STATE, INFINITE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, EnumWindows, FlashWindow, GetForegroundWindow, GetWindow, GetWindowThreadProcessId,
    InternalGetWindowText, IsIconic, SetForegroundWindow, SetWindowPos, ShowWindowAsync, ASFW_ANY, GW_OWNER,
    HWND_NOTOPMOST, HWND_TOPMOST, SWP_ASYNCWINDOWPOS, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SW_RESTORE, SW_SHOW,
};

const EVENT: &str = "Local\\PrismUIShow";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Une fenêtre Prism est déjà ouverte : elle est prévenue (elle revient au premier plan)
/// et l'appelant doit se fermer.
pub fn signal_existing() -> bool {
    let name = wide(EVENT);
    // SAFETY: poignée ouverte puis fermée ici.
    unsafe {
        let ev = OpenEventW(EVENT_MODIFY_STATE, 0, name.as_ptr());
        if ev.is_null() {
            return false;
        }
        // Nous venons d'être lancés par l'utilisateur : nous cédons notre droit de
        // passer au premier plan à la fenêtre déjà ouverte.
        AllowSetForegroundWindow(ASFW_ANY);
        SetEvent(ev);
        CloseHandle(ev);
        true
    }
}

/// Côté fenêtre ouverte : crée l'événement et, dans un fil, ramène la fenêtre `title`
/// de ce processus à chaque signal. Faux si une autre instance l'a déjà créé.
pub fn listen(title: &'static str) -> bool {
    let sddl = wide("D:(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;OW)(A;;0x00100002;;;IU)S:(ML;;NW;;;LW)");
    let name = wide(EVENT);
    // SAFETY: descripteur alloué par Windows puis libéré ; l'événement vit jusqu'à la
    // fin du processus (le fil d'attente le garde).
    unsafe {
        let mut sd = null_mut();
        let have_sd =
            ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.as_ptr(), SDDL_REVISION_1, &mut sd, null_mut())
                != 0;
        let sa = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: sd,
            bInheritHandle: 0,
        };
        // Réarmement automatique : un signal = un retour au premier plan.
        let ev = CreateEventW(if have_sd { &sa } else { null() }, 0, 0, name.as_ptr());
        let existed = GetLastError() == ERROR_ALREADY_EXISTS;
        if have_sd {
            LocalFree(sd as _);
        }
        if ev.is_null() || existed {
            if !ev.is_null() {
                CloseHandle(ev);
            }
            return false;
        }
        let h = ev as isize;
        std::thread::spawn(move || loop {
            if WaitForSingleObject(h as _, INFINITE) != 0 {
                return;
            }
            bring_to_front(title);
        });
        true
    }
}

struct Find {
    pid: u32,
    title: Vec<u16>,
    found: HWND,
}

unsafe extern "system" fn each(hwnd: HWND, lp: LPARAM) -> windows_sys::core::BOOL {
    let f = &mut *(lp as *mut Find);
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if pid != f.pid || !GetWindow(hwnd, GW_OWNER).is_null() {
        return 1;
    }
    let mut buf = [0u16; 64];
    // Sans message à la fenêtre : son fil (l'interface) peut être occupé.
    let n = InternalGetWindowText(hwnd, buf.as_mut_ptr(), buf.len() as i32).max(0) as usize;
    if buf[..n] == f.title[..f.title.len() - 1] {
        f.found = hwnd;
        return 0;
    }
    1
}

/// Fenêtre principale de ce processus : restaurée si réduite, puis au premier plan.
fn bring_to_front(title: &str) {
    let mut f = Find {
        // SAFETY: sans paramètre.
        pid: unsafe { GetCurrentProcessId() },
        title: wide(title),
        found: null_mut(),
    };
    // SAFETY: la structure vit pendant l'énumération (appel synchrone).
    unsafe {
        EnumWindows(Some(each), &mut f as *mut Find as LPARAM);
        if f.found.is_null() {
            return;
        }
        // Asynchrone : appelé hors du fil de l'interface, qui peut être occupé (attendre
        // sa réponse bloquait ce fil pour de bon, vu en VM avec la fenêtre réduite).
        ShowWindowAsync(f.found, if IsIconic(f.found) != 0 { SW_RESTORE } else { SW_SHOW });
        SetForegroundWindow(f.found);
        // Windows peut refuser le premier plan (lanceur qui ne l'avait pas lui-même) :
        // la fenêtre passe au moins devant les autres, sans prendre le clavier.
        if GetForegroundWindow() != f.found {
            let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW | SWP_ASYNCWINDOWPOS;
            SetWindowPos(f.found, HWND_TOPMOST, 0, 0, 0, 0, flags);
            SetWindowPos(f.found, HWND_NOTOPMOST, 0, 0, 0, 0, flags);
            FlashWindow(f.found, 1);
        }
    }
}
