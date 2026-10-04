# Measurements

Project rule: no gain is claimed without a measurement published here, along with its
protocol.

## Bench

- Windows 11 Enterprise (evaluation) 26H2 VM, build 26300: KVM, 6 cores, 12 GB,
  TPM 2.0 (swtpm), Secure Boot enabled.
- Fresh install, no third-party application, automatic sign-in.
- Sampling (`measure.ps1`): number of processes, running services, RAM used =
  total − available (`Win32_OperatingSystem`).
- Protocol: reboot, 4 to 5 minutes idle, then 3 to 9 samples taken 20 s apart.

## 1. Game Mode / `prism ram clean` — 2026-10-04

Idle 3 GB program (playing the role of "Word left open"), then `prism ram clean`:

| | Before | After |
|---|---|---|
| Free RAM | 5.3 GB (44%) | 8.3 GB (68%) |
| Freed | | **2.9 GB** |
| File cache (priority 5) | | intact |

Details of the mechanisms (modified list, priority 0): spec v0.1 §4.1.

## 2. Service debloat — 2026-10-04

| Configuration | Processes | Running services | RAM used (median) |
|---|---|---|---|
| Stock Windows, idle (after `restore`, 9 samples) | 113 | 78 | 2,065 MB |
| `sûr` level (safe; 3 samples) | 113 | 79 | 2,030 MB |
| `sûr` + `avancé` levels (safe + advanced; 9 samples) | 103 | 71 | 1,970 MB |

Reading:
- **`sûr` changes nothing measurable at idle.** On stock Windows, only 2 of its
  services are running (DiagTrack, TrkWks); the others are already stopped and only
  start on demand. Its value is privacy (telemetry, advertising ID, activity history),
  not performance.
- **`avancé`: −10 processes, −7 services, ≈ −95 MB (−5%).** It stops 5 services that
  actually run: SysMain, Windows Search, Print Spooler, PcaSvc, CDPSvc.
- `prism allege restore` puts back the 24 original values without failure; the state
  read back matches the stock state line by line, delayed starts included.

### Corrected error

A first baseline, taken right after installation, showed 124 processes and 2.66 GB:
Windows was still finishing its first-boot tasks. Compared with it, the `sûr` level
seemed to save 11 processes and 620 MB. That was wrong. The baseline kept is that of an
idle Windows, measured after restoring.

### Conclusion

Disabling Windows services brings little (≈ 100 MB, about ten processes).
The significant gains come from:
1. **Game Mode** (RAM of idle programs returned to the game: 2.9 GB measured);
2. **applications launched at startup**: measured in §3 (up to −1.23 GB).

## 3. Startup applications and Daily Mode — 2026-10-04

Same VM, with four common applications installed through winget (Discord, Steam, Epic,
Spotify). At startup: Discord (two entries), Steam, OneDrive and Edge preloading
(`MicrosoftEdgeAutoLaunch_…`). Samples taken under the same conditions (9 per
configuration, median).

| Configuration | Processes | Running services | RAM used | Difference from A |
|---|---|---|---|---|
| Stock Windows, without these apps (§2 baseline) | 113 | 78 | 2,065 MB | |
| **A** — apps installed, everything at startup | 142 | 86 | 3,257 MB | |
| **B** — `prism demarrage recommande` (Edge preloading off) | 133 | 84 | 2,943 MB | **−9 proc., −314 MB** |
| **C** — B + Discord, Steam, OneDrive off (`demarrage off`) | 116 | 80 | 2,023 MB | **−26 proc., −1.23 GB (−38%)** |
| **D** — A with `prism watch` (Daily Mode, everything kept at startup) | 144 | 88 | 2,957 MB | **−300 MB (−9%)** |

Reading:
- **Startup is the main lever.** A single preload (Edge) weighs more than the whole
  service debloat; by turning off optional apps, a fully equipped PC gets back to the
  level of a bare Windows while keeping its apps installed.
- **Daily Mode** saves 300 MB without closing or disabling anything: 16 idle apps eased
  (Widgets, Edge WebView2 processes, OneDrive, Steam, Discord…). For the measurement,
  the delays were shortened to 1 and 2 minutes (5 and 30 by default).
- `prism demarrage restore` put the 5 entries back exactly as they were; after a hard
  kill of `prism watch`, the next start restored the 32 Daily Mode settings.

## 4. Behavior verified in the VM (interactive session, as in real use)

| Check | Result |
|---|---|
| Daily Mode: idle 1 min → EcoQoS + low memory priority | ✓ 12 apps (test delays) |
| Idle 2 min → RAM returned | ✓ 1.5 GB → 1 MB |
| No false "activity resumed" caused by trimming | ✓ after fix (see spec v0.2 §1) |
| Game Mode: game detected under `C:\XboxGames` | ✓ |
| Game never touched | ✓ |
| Indexing (WSearch) stopped in game, restarted afterwards | ✓ |
| Game Mode: 48 actions then 35 restorations | ✓ 0 failures |
| Session 0 processes (services) never touched | ✓ after fix |

Side finding: Task Scheduler launches its tasks at `BelowNormal` priority with a low
memory priority, and their children inherit it.

## 5. Prism Bar, desktop widgets and per-app transparency — 2026-10-04

Same Windows 11 VM, run in the interactive user session.

| Check | Result |
|---|---|
| Bar docked at the bottom, Windows taskbar auto-hidden | ✓ |
| Maximised windows stop above the bar (work area reserved) | ✓ |
| Widgets: Start, open windows (active one highlighted), CPU and RAM with history, Game Mode, clock and date | ✓ |
| Live switch to the left edge, floating (10 px margin), rounded, 56 px — without restarting the bar | ✓ |
| Desktop icons and windows move right of the bar | ✓ |
| Notepad at 70 % opacity through a transparency rule (works with the new WinUI Notepad) | ✓ |
| Desktop widgets: clock with French date, system panel (CPU, RAM 2.5 / 12.0 GB, GPU, network), CPU graph | ✓ (GPU counters were available even in the VM) |
| `prism bar off`: Windows taskbar back (not auto-hidden), widgets gone, Notepad no longer layered | ✓ |
| Memory of `prism-bar.exe` with three desktop widgets | **15 MB** |

## What cannot be measured in a VM

- **FPS and micro-stutters**: this requires a real gaming PC, with a protocol published
  here.
- **Kernel anti-cheats** (Vanguard, FACEIT): they refuse to start in a virtual machine.
  Their compatibility is checked on a real PC. Prism does not touch them by design
  (`anticheat-rules.md`), and no service they depend on is in the catalog (test
  `anticheat_critical_services_are_protected`).
