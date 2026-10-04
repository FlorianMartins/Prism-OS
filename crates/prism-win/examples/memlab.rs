//! memlab — banc de mesure de la politique RAM sur un vrai Windows.
//!
//!   memlab lists                 listes mémoire (libre, zéro, modifiée, cache 0..7)
//!   memlab hog <Mo>              alloue et touche <Mo> de RAM privée puis attend
//!   memlab memprio <pid> <1-5>   priorité mémoire d'un processus
//!   memlab trim <pid>            rogne sa mémoire de travail
//!   memlab purge low|all         purge du cache en attente
//!   memlab flush                 écrit la liste modifiée (pages rognées -> cache)
//!   memlab ws <pid>              mémoire de travail et état d'un processus
//!   memlab cachefile <chemin> <Mo>  écrit puis relit un fichier (remplit le cache normal)

#[cfg(windows)]
fn main() {
    use prism_core::model::{human_bytes, MemPriority, PurgeScope, Target};
    use prism_core::plan::Action;
    use prism_core::platform::{Outcome, Platform};
    use prism_win::{flush_modified_list, memory_lists, proc_id, process_state, WindowsPlatform};

    let args: Vec<String> = std::env::args().skip(1).collect();
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut w = WindowsPlatform::new();
    let target = |pid: &str| {
        let pid: u32 = pid.parse().expect("pid");
        Target {
            id: proc_id(pid).expect("processus introuvable"),
            name: format!("pid {pid}"),
        }
    };
    let show = |o: Outcome| println!("{o:?}");
    match a.as_slice() {
        ["lists"] => match memory_lists() {
            Some(l) => {
                let sb: Vec<String> = l.standby_by_priority.iter().map(|b| (b >> 20).to_string()).collect();
                println!(
                    "free={} zero={} modified={} standby_mo[0..7]=[{}]",
                    human_bytes(l.free),
                    human_bytes(l.zero),
                    human_bytes(l.modified),
                    sb.join(",")
                );
            }
            None => println!("listes illisibles (administrateur requis)"),
        },
        ["hog", mb] => {
            let n: usize = mb.parse::<usize>().expect("Mo") << 20;
            let mut v = vec![0u8; n];
            for i in (0..n).step_by(4096) {
                v[i] = (i % 251) as u8 | 1;
            }
            println!("pid {} : {} Mo touchés, en attente", std::process::id(), n >> 20);
            loop {
                std::thread::sleep(std::time::Duration::from_secs(3600));
                std::hint::black_box(&v);
            }
        }
        ["memprio", pid, level] => {
            let to = MemPriority::from_level(level.parse().expect("niveau"));
            show(w.apply(&Action::MemoryPriority {
                target: target(pid),
                to,
            }));
        }
        ["trim", pid] => show(w.apply(&Action::TrimWorkingSet { target: target(pid) })),
        ["flush"] => println!("NTSTATUS {:#x}", flush_modified_list() as u32),
        ["purge", "low"] => show(w.apply(&Action::PurgeStandby { scope: PurgeScope::Low })),
        ["purge", "all"] => show(w.apply(&Action::PurgeStandby { scope: PurgeScope::All })),
        ["cachefile", path, mb] => {
            use std::io::{Read, Write};
            let n: usize = mb.parse::<usize>().expect("Mo");
            let chunk: Vec<u8> = (0..1usize << 20).map(|i| (i * 7 % 256) as u8).collect();
            let mut f = std::fs::File::create(path).expect("création");
            for _ in 0..n {
                f.write_all(&chunk).expect("écriture");
            }
            f.sync_all().expect("sync");
            drop(f);
            let mut buf = vec![0u8; 1 << 20];
            let mut f = std::fs::File::open(path).expect("ouverture");
            while f.read(&mut buf).expect("lecture") > 0 {}
            println!("{n} Mo écrits puis relus");
        }
        ["ws", pid] => {
            let t = target(pid);
            let snap = w.snapshot().expect("relevé");
            let p = snap.procs.iter().find(|p| p.id == t.id).expect("absent du relevé");
            println!("ws={} etat={:?}", human_bytes(p.working_set), process_state(&t));
        }
        _ => eprintln!("usage : voir l'en-tête de examples/memlab.rs"),
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("memlab ne fonctionne que sous Windows");
}
