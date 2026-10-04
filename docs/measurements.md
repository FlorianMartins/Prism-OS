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
- **`sûr` changes nothing measurable at idle** (79 vs 78 running services is
  sample-to-sample variation — trigger-started services come and go — not a service
  added by Prism). On stock Windows, only 2 of its
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

Two independent scenarios, both compared with **A** (lower is better):

**Scenario 1 — disable apps at startup** (each line builds on the previous one)

| Configuration | Processes | Running services | RAM used | vs A |
|---|---|---|---|---|
| **A** — apps installed, everything at startup | 142 | 86 | 3,257 MB | reference |
| **B** — `prism demarrage recommande` (Edge preloading off) | 133 | 84 | 2,943 MB | **−9 proc., −314 MB** |
| **C** — B + Discord, Steam, OneDrive off (`demarrage off`) | 116 | 80 | 2,023 MB | **−26 proc., −1.23 GB (−38%)** |

**Scenario 2 — keep every app at startup, let Daily Mode ease them**

| Configuration | Processes | Running services | RAM used | vs A |
|---|---|---|---|---|
| **A** — same reference as above | 142 | 86 | 3,257 MB | reference |
| **D** — A with `prism watch` running (Daily Mode) | 144 | 88 | 2,957 MB | **−300 MB (−9%)** |

For context, stock Windows without these apps (§2 baseline): 113 processes, 78 services,
2,065 MB.

How to read D: Daily Mode closes nothing, so the process count cannot go down. The +2
processes are `prism watch` itself plus normal sample-to-sample variation, and the
running-service count of an idle Windows moves by one or two between samples (§2 shows
the same effect). RAM is the figure Daily Mode acts on, and it goes down by 300 MB.

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
| Per-element transparency: the Windows "Run" dialog (`#32770`) at 60 % | ✓ (visibly translucent) |

## 6. Window effects — 2026-10-04

Same VM (WARP software rendering, no GPU), notepad window of about 900×575 px, default
settings (genie, 350 ms, 60 Hz). Each figure is read from `prism fx stats`, which
records every animation the bar plays.

| Trigger | Latency to first frame | Time per frame (avg / max) | Dropped frames |
|---|---|---|---|
| Click on the bar button → minimize | 3.5–4.8 ms | 1.5–1.6 ms / 6.1 ms | 0 / 49 |
| Minimize by Windows (title-bar button, shortcut) | 4.5–5.6 ms | 1.4–1.5 ms / 4.6 ms | 0 / 52 |
| Restore by Windows | 4.3–4.6 ms | 1.4 ms / 3.1 ms | 0 / 51 |
| `prism fx demo` (restore, close, open) | 2–3 ms | ≈ 2.0 ms | 0 |
| **Jelly while dragging** (5 drags, slow and fast, ≈ 1,000 px/s for the fastest) | 38–54 ms, see below | 2.2–2.9 ms / 9.5 ms | 0 in every drag (143 to 386 frames each) |

Jelly while dragging: the 38–54 ms before the first jelly frame is the copy of the window
(33–49 ms in this VM). During that time the real window is still shown and already follows
the mouse, so nothing lags: the jelly replaces it once ready. Frames written by the bar
itself during a drag (debug mode) show the far side of the window trailing and bending,
then settling about half a second after release.

- **Latency is below one frame** (16.7 ms at 60 Hz). The expensive step is copying the
  window (`PrintWindow`, 18–46 ms in this VM). It never sits between the action and the
  effect: the copy is taken when the window gets focus, or when the mouse button goes
  down on the bar (a click lasts about 100 ms), and is ready when the effect starts.
- **Cost**: a frame takes 1.4–2 ms on one CPU core, so a 350 ms effect uses about 10 % of
  one core for a third of a second. The bar uses 19 MB with effects on (15 MB without).
- Animations that arrive more than 250 ms late are skipped instead of played late.

Bugs found by these measurements and fixed:

- An effect triggered by Prism itself was also received back from Windows and animated a
  second time, up to 501 ms late.
- A minimized window still counts as visible and receives focus. It was copied in its
  parked 160×28 shape and restored from that tiny image. Minimized windows are no longer
  copied.
- Waiting 40 ms for a new window to finish drawing made the open effect start 167 ms
  late. The wait was removed.
- A window hidden at opacity 0 no longer receives the mouse: the drag stopped dead the
  moment the jelly started. Windows are now hidden at opacity 1/255 (invisible to the
  eye, still a mouse target).
- The end-of-move event did not always arrive, which left the window hidden. The end of
  a drag is now read from the window's own thread (`GetGUIThreadInfo`, flag
  `GUI_INMOVESIZE`) at every frame, and a bar that is killed during an effect makes the
  window visible again at its next start.

Known limit: a minimize started from the window itself (title-bar button, shortcut) is
animated from the copy taken when the window last got focus — Windows has already
minimized it when the event arrives. If the window's content changed since, the first
frames show the older content. Minimizing from the Prism Bar always uses a fresh copy.

## What cannot be measured in a VM

- **FPS and micro-stutters**: this requires a real gaming PC, with a protocol published
  here.
- **Kernel anti-cheats** (Vanguard, FACEIT): they refuse to start in a virtual machine.
  Their compatibility is checked on a real PC. Prism does not touch them by design
  (`anticheat-rules.md`), and no service they depend on is in the catalog (test
  `anticheat_critical_services_are_protected`).
