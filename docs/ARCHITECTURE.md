# Prism OS Architecture

## What Prism OS is

A **gaming and cybersecurity layer on top of a real Windows**: the user's own, with
their own license. The Windows kernel stays untouched, so anti-cheats see a genuine
Windows (Secure Boot, TPM, VBS). Prism changes everything above it:

- resource management (Game Mode, RAM, priorities, power) — v0.1/v0.2;
- the interface (dashboard, game launcher, console mode, Prism Bar, widgets, appearance) — v0.3;
- window effects (genie lamp, jelly, zoom) drawn by Prism with official APIs — v0.4;
- privacy (telemetry turned off through the official mechanisms + firewall) — v0.5 ✓;
- usage profiles and on-demand cybersecurity tools.

Prism **is not** a modified Windows ISO, which the license forbids redistributing:
it is software applied to the Windows that is already installed, and every setting is
reversible.

The from-scratch multiple-kernel project has become **MultiKernel**
(`FlorianMartins/MultiKernel`), a separate project.

## Layout

```
crates/
  prism-core/   pure logic, no Windows dependency — tested on Linux
                 config (TOML) · classification · plan · engine · journal · RAM · tools
                 daily (Daily Mode) · cores (P/E, X3D) · allege · demarrage
                 library (installed games) · apparence (Appearance) · bar (Prism Bar)
                 etat (live state shared with the interface)
                 cadence (engine: light PID check, full scan only when useful)
                 noyau (kernel anti-cheat plan: detection, tool standby, enter/exit)
                 webview (background WebView2 engines: owner app, delay, recreation)
                 rapport (memory report: layout and advice; collected by prism-win)
  prism-win/    execution on Windows (windows-sys): process snapshot, applying and
                 restoring each lever, startup entries, services/policies/scheduled
                 tasks, appearance settings, game library, metrics, Prism Bar window
  prism/        binary prism.exe: CLI + watch loop (automatic Game Mode)
                 binary prism-bar.exe: Prism Bar (native Win32 taskbar)
  prism-ui/     binary prism-ui.exe: graphical interface (egui)
config/
  default.toml     profiles, protected/companion/game lists, tool catalog
  allegement.toml  debloat catalog (services, policies, scheduled tasks, preinstalled
                   apps, protected services/tasks/apps)
  apparence.toml   appearance settings and presets
  demarrage.toml   startup-app recommendations
```

The key boundary is the `Platform` trait (`prism-core/src/platform.rs`):
- the core **decides** (what to do, to whom, in which order, how to undo it);
- the platform **executes** and returns the original value for the journal.

All the decision logic is therefore testable without Windows, and the Windows part is
thin, which makes it auditable. The interface follows the same pattern: `prism-ui` only
talks to its `Backend` trait (real Windows, or a mock for tests and screenshots), and the
Windows backend uses the same functions and journals as `prism.exe`, so a setting changed
in the interface can be undone by `prism … restore`, and vice versa.

## Roadmap

| Version | Content |
|---|---|
| **v0.1** ✓ | Game Mode engine + RAM policy + profiles + cyber catalog, CLI, Windows CI — [spec](specs/v0.1-game-mode.md) |
| **v0.2** ✓ | permanent Daily Mode, P/E and X3D cores, paused services, freezing, RAM monitoring, startup apps, game settings, reversible debloat — [spec](specs/v0.2-process-management.md) |
| **v0.3** ✓ (partial) | interface: `prism-ui` dashboard, game launcher + console mode, Store app startup tasks, Appearance (official Windows animation/effects/theme settings with presets), Prism Bar (native Win32 taskbar on any screen edge, one per screen), desktop widgets, per-app transparency rules, scheduled-task debloat — [spec](specs/v0.3-interface.md). Still planned: Prism as a Windows service, tiling window manager |
| **v0.4** ✓ | window effects: 7 effects (genie, squash, jelly, zoom, fade, 3D tilt, fall apart) per action on minimize/restore/open/close, glide/elastic/crossfade on maximize and snap, jelly while dragging, software-rendered in the Prism Bar's click-through overlay, measured per animation (`prism fx stats`); per-element transparency (menus, dropdowns, tooltips, dialogs) — [spec](specs/v0.4-effects.md) |
| **v0.5** ✓ | privacy: 21 official policies / Settings options and 3 Windows Firewall rules (CompatTelRunner, DeviceCensus, error reports) at two levels, read-only view of what debloat already does, dashboard with live telemetry connections — [spec](specs/v0.5-privacy.md) |
| **v0.6** ✓ | customization: colour themes for the bar, widgets and app (8 presets, custom accent, WCAG contrast tested), export/import of the whole setup, tiling window manager (4 layouts, per screen, shortcuts, exact undo) — [spec](specs/v0.6-customization.md) |
| **v0.7** (partial) | MSI installer built from Linux in CI (Start menu, PATH, in-place upgrades) whose uninstall puts back everything Prism changed ✓; release workflow (tag → MSI + SHA256SUMS on GitHub) and `prism maj` updates verified by SHA-256 ✓ — [spec](specs/v0.7-installer.md). Still to do: code signing, published FPS measurements |
| **v0.8** ✓ | settings that apply without administrator rights, administrator rights without prompt (on-demand task), start with Windows, Windows taskbar fully replaced by the Prism Bar |
| **v0.9** ✓ | CPU: bar 19.8 % → 0.26 % of a core (tiling loop fixed), two-speed engine; Extreme debloat level (services on demand, preinstalled apps removed and re-registered offline on undo); automatic, configurable kernel anti-cheat plan (Extreme services back for the game, cybersecurity tools on standby) |

## Principles

- **Lightweight**: Rust, no runtime, no heavy service; the engine sleeps between two
  passes and the interface closes during gameplay.
- **Measure before claiming**: no "+X% FPS" without a published measurement
  (see [`measurements.md`](measurements.md)).
- **Reversible**: every change goes through the journal.
- **Anti-cheat first**: see [`anticheat-rules.md`](anticheat-rules.md).
