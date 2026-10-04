# Prism OS — User Guide

Prism OS is a gaming and cybersecurity layer for **your own Windows 10/11 installation**.
It does not replace Windows and never touches its kernel, so anti-cheats (Vanguard,
FACEIT, Easy Anti-Cheat, BattlEye) keep seeing a genuine Windows. Every change Prism
makes is recorded and can be undone.

> The command-line interface and the app are currently in French. This guide gives
> the exact command names (some are French words, e.g. `demarrage` = startup,
> `allege` = debloat, `apparence` = appearance, `jeux` = games).

---

## 1. What you get

| Program | What it is | Size |
|---|---|---|
| `prism.exe` | Command-line tool and the background engine (`prism watch`) | ~1.3 MB |
| `prism-ui.exe` | Graphical app: dashboard, games, startup, debloat, appearance, tools | ~11 MB |
| `prism-bar.exe` | Prism Bar: your own taskbar plus desktop widgets | ~0.5 MB |

Measured on Windows 11: `prism watch` uses about **3 MB** of RAM, the app about 58 MB
(close it while you play), the bar a few MB.

## 2. Install

1. Download `prism.exe`, `prism-ui.exe` and `prism-bar.exe` (CI artifact
   `prism-windows-x64`, or build them, see the README) and put them in one folder,
   for example `C:\Program Files\Prism`.
2. Open a terminal **as administrator** in that folder.
3. Start the engine automatically when you log in:
   ```powershell
   prism autostart on
   ```
4. Optional — start the Prism Bar automatically too:
   ```powershell
   prism bar autostart on
   ```

Prism must run **in your user session** (that is what `autostart` sets up). If it runs
in the services session (for example over SSH), it refuses to touch any process and
`prism status` tells you so.

To uninstall: `prism autostart off`, `prism bar autostart off`, then restore what you
changed (section 11) and delete the folder.

## 3. First steps

```powershell
prism status        # profile, memory, CPU layout, detected games, anti-cheat conflicts
prism demo          # a simulated gaming session, end to end (works on any computer)
prism-ui            # the graphical app
```

## 4. Profiles

| Profile | For | Daily Mode | Game Mode |
|---|---|---|---|
| `gaming` (default) | Playing | apps idle 5 min eased, RAM returned after 30 min | full: priorities, cores, RAM, indexing paused, high-performance power plan, WSL stopped |
| `balanced` | Everyday use | idle 10 min / 60 min | games first, RAM and power untouched |
| `cyber` | Security work | idle 15 min / 90 min (heavy tools never slowed) | off; WSL/Kali left alone |

```powershell
prism profile           # list profiles, the active one is marked
prism profile cyber     # switch (the running engine picks it up within 2 seconds)
```

## 5. Daily Mode and Game Mode (`prism watch`)

`prism watch` runs both modes at once (this is what `autostart` launches):

- **Daily Mode, always on.** A background app that has done nothing for a while (no CPU
  time, not in the foreground) is moved to efficiency cores with low power and low
  memory priority; later its RAM is returned to Windows. As soon as you switch back to
  it, or it starts working again, everything is given back immediately.
- **Game Mode, automatic.** When a game starts (Steam, Epic, GOG, Xbox, Riot, EA,
  Ubisoft, Battle.net folders, plus every game found in your libraries), background
  apps step back, their RAM is freed without touching the game's file cache, Windows
  Search indexing pauses, and on hybrid Intel CPUs or dual-CCD Ryzen X3D the
  background is moved off the cores the game benefits from. **The game process itself
  is never touched.** Everything is restored when the game closes.

Optional: list apps to **freeze** during games in `config.toml`
(`suspend_in_game = ["onedrive.exe"]`). A frozen app does not respond at all until the
game ends — do not put your voice chat or music there.

If Prism is killed or the PC crashes mid-game, the next start of `prism watch` restores
every recorded setting automatically.

## 6. Memory

```powershell
prism ram               # free memory, cache, how much is reclaimable
prism ram clean         # return the RAM of idle background apps now
prism ram clean --deep  # also empty the whole file cache (rarely useful)
prism top               # what is using CPU and RAM right now
```

`ram clean` frees the memory of apps you left open but are not using (measured: 2.9 GB
returned in a test VM) and keeps the cache that holds your game's files.

## 7. Startup apps (`prism demarrage`)

Disabling an app at startup does not uninstall it: it simply starts when you open it.
Prism uses the same mechanism as Task Manager's "Disable" button.

```powershell
prism demarrage                  # list with advice: "à désactiver" (disable), "optionnel", "à garder" (keep), "protégé" (protected)
prism demarrage recommande       # disable everything marked "à désactiver"
prism demarrage off Discord      # disable one entry
prism demarrage on Discord       # re-enable it
prism demarrage restore          # put everything back as it was
```

Security tools and anti-cheat entries (Vanguard, EAC, BattlEye, FACEIT) are protected
and cannot be disabled. Microsoft Store app startup tasks (for example the new Teams)
are included.

Measured in a test VM with Discord, Steam, OneDrive and Edge: **−1.23 GB and −26
processes** when the optional apps were disabled too.

## 8. Debloat (`prism allege`, administrator)

```powershell
prism allege                 # catalogue with the current state of every entry
prism allege apply           # "sur" (safe): telemetry, useless services, Edge background, telemetry tasks
prism allege apply avance    # "advanced": SysMain, Search indexing, Print Spooler… each with its trade-off
prism allege apply jeu       # gaming settings: Windows Game Mode, GPU scheduling, windowed games, mouse acceleration off
prism allege restore         # put back every original value
```

72 services that anti-cheats, updates and security need are protected and can never be
changed, even by editing the catalogue. Be realistic: disabling services saves about
100 MB; startup apps and Daily Mode are where the big gains are.

## 9. Appearance (`prism apparence`)

Official Windows settings only: window animations (minimize/maximize, open/close, menus,
tooltip fade, combo boxes, smooth scrolling), shadows, transparency, font smoothing,
dark/light theme, accent colour on title bars, submenu delay, taskbar icon alignment.

```powershell
prism apparence                          # every setting with its current value
prism apparence performance              # preset: everything instant, lightest
prism apparence fluide                   # preset: all animations, snappier menus
prism apparence set menu_delay "Vif (100 ms)"
prism apparence restore                  # back to how Windows was before Prism
```

Free-form animations like on Hyprland (custom curves, bouncing windows) would require
injecting code into the Windows compositor. Prism does not do that, by design, to stay
safe with anti-cheats.

## 10. Prism Bar and desktop widgets

The Prism Bar replaces the Windows taskbar visually and can sit on **any edge**.

```powershell
prism bar on        # start it
prism bar off       # stop it (the Windows taskbar comes back)
```

Configure it live in **prism-ui → Apparence**:

- position (top, bottom, left, right), thickness, margin (floating bar), opacity,
  rounded corners;
- hide the Windows taskbar while the bar runs (uses Windows' own auto-hide option and
  restores it on exit);
- hide during fullscreen games and videos;
- bar widgets: Start button, open windows (click to switch), CPU, RAM, GPU, network,
  Game Mode indicator, clock;
- **desktop widgets**: clock, system panel, CPU/RAM/GPU graphs, network speed. Drag them
  with the mouse; their position is remembered;
- **per-app transparency**: e.g. `windowsterminal.exe` at 90 %. Never applied to a game
  or a fullscreen window.

Settings are stored in `%ProgramData%\Prism\bar.json` and applied within a second.

## 11. Undo everything

| What | Command |
|---|---|
| Game Mode / Daily Mode | stop `prism watch` (Ctrl-C) — restored automatically, also after a crash |
| Startup apps | `prism demarrage restore` |
| Debloat | `prism allege restore` |
| Appearance | `prism apparence restore` |
| Prism Bar, transparency | `prism bar off` |

Journals live in `%ProgramData%\Prism\` (`journal.json`, `quotidien.json`,
`demarrage.json`, `allegement.json`, `apparence.json`).

## 12. Games and the console mode

```powershell
prism jeux                       # installed games (Steam, Epic, GOG, Battle.net)
prism jeux lancer elden ring     # launch through its store
```

In prism-ui, **Jeux → Mode console** opens a fullscreen launcher you can drive with
the arrow keys and Enter (Esc to leave).

## 13. Cybersecurity tools

```powershell
prism tools                  # packs: reseau (network), web, reverse, kali
prism tools reverse          # what a pack installs, with the exact commands
prism tools install kali     # Kali Linux under WSL + essential tools (nmap, sqlmap, hashcat…)
```

Nothing is installed by default and nothing runs while you play (the gaming profile
stops WSL when a game starts). Tools that can upset anti-cheats (debuggers, kernel-driver
tools) are flagged, and `prism status` warns if one is open.

## 14. Configuration

```powershell
prism config init     # writes an editable copy to %ProgramData%\Prism\config.toml
prism config check    # validates it
prism config path
```

Invalid files are refused with a clear message rather than half-applied.

## 15. Troubleshooting

| Symptom | Cause / fix |
|---|---|
| `prism status` says Prism runs in the services session | Start it from your own session: `prism autostart on` |
| "droits administrateur requis" (administrator rights required) | Open the terminal as administrator, or use "Relancer en administrateur" in prism-ui |
| Widgets setting refused by Windows | Recent Windows 11 builds lock some settings; Prism does not bypass Windows protections |
| GPU widget shows "—" | No GPU performance counters (virtual machine or basic display driver) |
| A game refuses to start because of a tool | Close the flagged debugger/kernel-driver tool shown by `prism status` |
