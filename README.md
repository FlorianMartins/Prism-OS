# Prism OS

**The gaming and cybersecurity layer for a genuine Windows.**

Prism installs on top of *your* Windows, with your licence, and leaves its kernel
untouched: anti-cheats (Vanguard, FACEIT, Easy Anti-Cheat, BattlEye) still see a genuine
Windows. Prism takes over everything above it: resources, startup apps, debloat,
appearance, a taskbar you can put on any edge, desktop widgets and on-demand security
tools. Every change is journaled and reversible.

> The from-scratch multikernel project that used to carry this name is now
> [MultiKernel](https://github.com/FlorianMartins/MultiKernel).

**Start here: [User Guide](docs/USER_GUIDE.md).**

## Features

- **Daily Mode, always on** — idle background apps move to efficiency cores with low
  power and low memory priority, then give their RAM back; the moment you return to an
  app, it gets everything back.
- **Game Mode, automatic** — when a game starts, the background steps back, idle apps'
  RAM is freed without touching the game's file cache, Search indexing pauses, and on
  Intel hybrid / Ryzen X3D CPUs the background leaves the cores the game benefits from.
  The game process itself is never touched.
- **Startup apps** (`prism demarrage`) — advice and reversible disabling, the same
  mechanism as Task Manager, including Microsoft Store apps.
- **Debloat** (`prism allege`) — telemetry services and scheduled tasks, Edge/Brave/Chrome
  background processes and memory saver, Recall, delivery-optimisation upload, plus gaming settings;
  72 services that anti-cheats and updates need are protected.
- **Privacy** (`prism vie-privee`) — Windows telemetry, ads and suggestions, Bing in
  Start, typing collection, Edge reporting turned off through official policies, plus
  Windows Firewall rules that cut telemetry programs off; *recommended* or
  *strict* levels, a dashboard with a live "telemetry talking right now" panel.
- **Appearance** (`prism apparence`) — official Windows animations, effects and theme
  settings with presets (Performance max, Fluide, original settings).
- **Prism Bar** — a native taskbar on any edge (top, bottom, left, right) and on every
  screen (each bar shows that screen's windows, or all of them), adjustable
  thickness, floating margin, opacity and widgets (CPU, RAM, GPU, network, Game Mode,
  clock); hides during fullscreen games.
- **Themes** — Prism, Nord, Dracula, Catppuccin, Gruvbox, Tokyo Night, a red gaming
  theme and a light one, or your own accent colour, for the bar, widgets and app
  (every preset is tested for readable contrast).
- **Export / import** (`prism config export|import`) — your whole setup in one file.
- **Tiling window manager** — master + stack, columns, spiral or monocle, per screen,
  with gaps and keyboard shortcuts (Win+Ctrl+Alt); never games or fullscreen windows;
  everything back in place when turned off.
- **Desktop widgets and transparency** — Conky-style widgets; translucent app windows,
  menus, dropdown lists, tooltips and dialogs, each with its own opacity (never games).
- **Window effects, KDE/Bazzite-style** — a choice per action: genie lamp, squash,
  jelly, zoom, fade, 3D tilt or fall apart for minimize, restore, open and close; glide,
  elastic or crossfade when a window is maximized or snapped to an edge; jelly
  ("wobbly") windows while you drag them. Drawn by Prism with official APIs; adjustable
  duration and intensity.
- **App** (`prism-ui`) — dashboard and every setting above.
- **Cyber tools on demand** — Wireshark, Burp, ZAP, Sysinternals, x64dbg, Kali under WSL
  (nmap, sqlmap, hashcat…); nothing runs while you play, anti-cheat-hostile tools are
  flagged.
- **Anti-cheat compatible by construction** — no driver, no injection, no system file
  modified. See the [rules](docs/anticheat-rules.md).

## Measured results

From a Windows 11 test VM ([details and protocol](docs/measurements.md)):

| Action | Result |
|---|---|
| `prism ram clean` with an idle 3 GB app | 2.9 GB returned (44 % → 68 % free), game file cache intact |
| Startup: Edge preload disabled | −9 processes, −314 MB |
| Startup: + Discord, Steam, OneDrive disabled | −26 processes, −1.23 GB (−38 %) |
| Daily Mode with every app kept at startup | −300 MB |
| Debloat "advanced" services | −10 processes, ≈ −95 MB |
| `prism watch` memory footprint | ≈ 3 MB |
| Prism Bar with three desktop widgets | ≈ 15 MB |
| Window effects: latency to first frame / time per frame | 3–6 ms / 1.5 ms, 0 dropped frames |
| Jelly while dragging | 2.7 ms per frame, 0 dropped frames |
| Maximize / snap animation: latency / time per frame | 2–5 ms / 1.3–2.7 ms, 0 dropped frames |

FPS and anti-cheat compatibility cannot be measured in a VM; they will be measured on
real gaming hardware.

## Quick start

**Download (permanent link, always the latest version):** <https://hivey.be/prism> —
[Prism-Setup.exe](https://github.com/FlorianMartins/Prism-OS/releases/latest/download/Prism-Setup.exe)

Install with `prism-<version>-x64.msi` (GitHub Releases, or CI artifact
`prism-installer`): Start menu entry, `prism` in every terminal, and uninstalling from
*Installed apps* puts back everything Prism changed. `prism maj installer` updates to
the latest release after checking its SHA-256 fingerprint.

```powershell
prism status               # what Prism sees: profile, memory, CPU layout, games, conflicts
prism autostart on         # start the engine at logon (administrator terminal)
prism bar on               # start the Prism Bar
prism-ui                   # the app
prism demo                 # a simulated gaming session (any OS)
```

All commands: [User Guide](docs/USER_GUIDE.md).

## Build

```bash
cargo test --workspace                                                    # anywhere
cargo build --release -p prism -p prism-ui --target x86_64-pc-windows-gnu # Windows binaries from Linux (mingw-w64)
```

On Windows: `cargo build --release -p prism -p prism-ui`. UI screenshots are rendered
off-screen by the tests into `target/ui-shots/` (needs a Vulkan driver; on Linux,
`mesa-vulkan-drivers`).

## Layout

| Path | Role |
|---|---|
| `crates/prism-core` | decisions, tested without Windows: classification, plans, RAM policy, journals, Daily Mode, cores, debloat, startup, appearance, bar layout, library |
| `crates/prism-win` | Windows execution (documented APIs, `windows-sys`), Prism Bar, metrics |
| `crates/prism` | `prism.exe` (CLI + engine) and `prism-bar.exe` |
| `crates/prism-ui` | `prism-ui.exe` (egui/wgpu) |
| `config/` | profiles, catalogues (debloat, startup, appearance) |
| `docs/` | [user guide](docs/USER_GUIDE.md), [architecture](docs/ARCHITECTURE.md), [specs](docs/specs/), [anti-cheat rules](docs/anticheat-rules.md), [measurements](docs/measurements.md) |

## Roadmap

v0.1 engine ✓ · v0.2 process management ✓ · v0.3 interface and customisation ✓ ·
v0.4 window effects ✓ · v0.5 privacy ✓ · v0.6 themes, export and tiling ✓ ·
v0.7 installer and updates ✓ ·
next: code signing,
FPS measurements on real hardware. Details:
[ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Licence

Prism OS is free software under the [MIT licence](LICENSE). Unless stated otherwise,
any contribution submitted to the project is licensed under the same terms.
