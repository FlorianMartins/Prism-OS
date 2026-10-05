# Prism OS — User Guide

Prism OS is a gaming and cybersecurity layer for **your own Windows 10/11 installation**.
It does not replace Windows and never touches its kernel, so anti-cheats (Vanguard,
FACEIT, Easy Anti-Cheat, BattlEye) keep seeing a genuine Windows. Every change Prism
makes is recorded and can be undone.

> The command-line interface and the app are currently in French. This guide gives
> the exact command names (some are French words, e.g. `demarrage` = startup,
> `allege` = debloat, `vie-privee` = privacy, `apparence` = appearance, `jeux` = games).

---

## 1. What you get

| Program | What it is | Size |
|---|---|---|
| `prism.exe` | Command-line tool and the background engine (`prism watch`) | ~1.3 MB |
| `prism-ui.exe` | Graphical app: dashboard, games, startup, debloat, privacy, appearance, tools | ~11 MB |
| `prism-bar.exe` | Prism Bar: your own taskbar plus desktop widgets | ~0.5 MB |

Measured on Windows 11: `prism watch` uses about **3 MB** of RAM, the Prism Bar with its
widgets about **15 MB**, the app about 58 MB (close it while you play).

## 2. Install

**Permanent download link** (always the latest version — this address never
changes): <https://hivey.be/prism> (short), or <https://github.com/FlorianMartins/Prism-OS/releases/latest/download/Prism-Setup.exe>

1. Download **`Prism-Setup.exe`** (link above, or `Prism-Setup-<version>.exe` from a
   given GitHub release) and run it: an
   animated installer (the Prism logo builds itself and spins while installing) lets
   you **choose the install folder** (default `C:\Program Files\Prism`), shows the
   installation step by step, and offers *Lancer Prism* at the end. The plain
   `prism-<version>-x64.msi` is also published, with its own wizard (folder choice,
   *Lancer Prism*), and for silent installs (`msiexec /i … /qn`). Prism adds a **Prism** entry to
   the Start menu (the app), adds the folder to the system `PATH` (so `prism` works in
   any terminal) and registers *Prism OS* in **Settings › Apps › Installed apps**.
   Installing a newer version upgrades in place. When installation ends, the Prism app
   opens by itself; on first launch it offers **Lancer la Prism Bar et activer les
   effets** (start the bar and turn the effects on) so you see the difference at once.
   Windows SmartScreen warns on first run because Prism is not code-signed yet: choose
   *More info* › *Run anyway*.
2. Open a terminal **as administrator**.
3. Start with Windows is **on by default** after installing. To switch it off or back on:
   ```powershell
   prism autostart off
   prism autostart on
   ```

**Administrator rights without prompts**: the installer creates an on-demand task,
*Prism (admin)*, for your account; opening the app without rights relaunches it through
that task, with administrator rights and no confirmation window. **Start with Windows**
(on by default; *Prism au démarrage de Windows* switch on the dashboard,
`prism autostart on|off`): the engine — Daily Mode, Game Mode, automatic RAM cleaning —
starts **when Windows boots, before the sign-in screen**, under the system account (a
scheduled task with no time limit, normal priority, also on battery, restarted if it
stops); the Prism Bar starts at every sign-in, for any user. Measured in the VM: engine
running as `NT AUTHORITY\SYSTEM` 5 seconds after boot. Running as the system account,
the engine acts on the session of whoever is signed in at the console and reads that
person's settings (`%LOCALAPPDATA%\Prism`); at the sign-in screen it touches no app.
A single engine runs on the machine at a time.

**Where settings live**: your own settings (bar, theme, effects, tiling, appearance)
are in `%LOCALAPPDATA%\Prism`, always writable by you; journals of machine-wide changes
(debloat, privacy, engine) stay in `%ProgramData%\Prism`, read-only for standard
accounts. (Up to v0.7.2 everything was in `%ProgramData%\Prism`, read-only for the app
when it ran without rights: settings were silently not saved. They are moved
automatically.)

Started from the services session another way (for example over SSH) without anyone
signed in at the console, the engine touches no process.

**Uninstall** from *Settings › Apps › Installed apps › Prism OS*, the **Désinstaller
Prism** entry of the Start menu, or the **Désinstaller Prism** button at the bottom of
the app's dashboard. Before removing the
files, the uninstaller puts back everything Prism changed: it stops the engine and
applies its journals, stops the Prism Bar (the Windows taskbar comes back, translucent
and tiled windows return to normal), restores privacy, debloat, appearance and startup
apps, removes the scheduled tasks and the `PATH` entry. The uninstaller runs as the
system account; the part that belongs to your session (your bar, your settings) is run
in your session, with administrator rights, through a one-off scheduled task that is
deleted afterwards. Your Prism settings files stay in `%ProgramData%\Prism`.

The same clean-up is available on its own: `prism desinstaller` (administrator).

### Updates

```powershell
prism maj             # is there a newer version? (GitHub Releases)
prism maj installer   # download it, verify it, install it (administrator)
```

`prism maj installer` downloads the installer and the published `SHA256SUMS`, checks
the installer's SHA-256 fingerprint — a file that does not match is deleted and nothing
is installed — then hands over to a temporary copy of itself (a running program cannot
be replaced): it stops the Prism Bar and the engine (applying the engine's journal),
installs the new version as an upgrade (your settings and journals are kept) and starts
again what was running. Prism never updates itself without you asking.

## 3. First steps

```powershell
prism status        # profile, memory, CPU layout, detected games, anti-cheat conflicts
prism demo          # a simulated gaming session, end to end (works on any computer)
prism-ui            # the graphical app
```

### The app's look

Cards react to the mouse: they tilt towards the pointer, a soft light follows it, the
shadow shifts and the border lights up cyan → violet; animations only run while you
hover, so the app does not redraw on its own. Every page sits on a subtle tech backdrop (fine grid,
soft cyan and violet glows); cards arrive in a short cascade when you open a page; a
glowing line unfolds under the page title; the active menu entry has a glowing marker;
messages slide in as floating notifications at the bottom right and fade out; an
indicator next to the title shows an action running in the background. The dashboard opens with live gauges for
the processor, memory and graphics card (same measurements as the Prism Bar).

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
  it, or it starts working again, everything is given back immediately. The app you
  are using is spared as a whole: every process of the same program (each browser tab,
  each Electron window) and every process it started (an embedded WebView), not just
  the one that owns the window.
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

**Memory compression** (*Allègement* page, top): when Prism gives back the RAM of an
idle app, Windows can compress part of it and keep it in memory (the *Memory
Compression* process), still counted as used. Switching compression off sends it to the
page file instead. Measured in the VM: trimming a 1.5 GB app lowered used memory by
1.67 GB and *Memory Compression* grew by only 106 MB — so the gain is that share (up to
a few hundred MB on a PC where *Memory Compression* is large), not gigabytes. Suggested
from 16 GB of RAM; takes effect at the next restart; *Tout restaurer* and uninstalling
put the original setting back.

**Automatic RAM cleaning** (dashboard, *Nettoyage automatique*, on by default): the engine
runs the same action as *Libérer la RAM des applis inactives* by itself — every 15
minutes and whenever used memory goes above 75 % (both adjustable: 5–120 min, 50–95 %),
at least 2 minutes apart, never during a game. Settings in
`%LOCALAPPDATA%\Prism\ram-auto.json`.

Daily Mode also covers game companions (Discord, Steam, NVIDIA overlay…) outside games:
when idle they get low memory priority and give their RAM back after the delay (10 min
in the gaming profile), without being slowed down (no EcoQoS, no efficiency cores) so a
voice call stays smooth; they are restored as soon as a game starts. Before, they were
never touched — on a real PC they were the biggest consumers (Discord 1.5 GB, Steam
1.15 GB).

**Memory report** (`prism rapport`, or *Rapport mémoire* on the dashboard): where this
PC's memory goes, as a plain-text file saved on the Desktop (and in
`%LOCALAPPDATA%\Prism`) and opened in Notepad. It contains no personal data — no user
name, no computer name, no file paths: only program and service names and figures — so
it can be sent as is when asking for help. Sections:

- machine (Windows edition and build, processor, uptime) and memory: installed,
  usable, reserved by hardware (integrated graphics, BIOS), in use, available, cache,
  modified, free, compressed, kernel pools, commit;
- advice computed from the figures: hardware-reserved memory, a kernel pool far above
  normal (the classic sign of a leaking driver), memory compression, commit near its
  limit, WebViews, the heaviest apps, many startup apps, and the cache — which Windows
  hands back instantly and is not lost memory;
- apps (processes added up) ranked by memory in RAM, with their private memory —
  private memory paged out to disk does not occupy RAM (measured: a Notepad with 612 MB
  private and 5 MB in RAM);
- service hosts (`svchost`) with the services each one runs, WebViews per app, startup
  apps, and Prism's own state.

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
prism allege apply           # "sur" (safe): telemetry, useless services, Edge/Brave/Chrome background, telemetry tasks
prism allege apply avance    # "advanced": SysMain, Search indexing, Print Spooler, browser memory saver… each with its trade-off
prism allege apply jeu       # gaming settings: Windows Game Mode, GPU scheduling, windowed games, mouse acceleration off
prism allege apply extreme   # "extreme": background services on demand, Copilot, preinstalled apps removed
prism allege restore         # put back every original value
```

72 services that anti-cheats, updates and security need are protected and can never be
changed, even by editing the catalogue. Be realistic: disabling services saves about
100 MB; startup apps and Daily Mode are where the big gains are.

**Browsers**: the safe level stops Edge, Brave and Chrome from staying in the background
once their last window is closed. The advanced level turns on their memory saver
(sleeping tabs in Edge): inactive tabs release their memory and reload when you click
them. Both use the browsers' official policies, so the browser shows "managed by your
organisation" — untick the item to remove the policy.

**One item at a time**: every line of the *Allègement* page has its own checkbox —
tick to apply that single item, untick to put its original value back (only for items
Prism changed; an item that was already that way before Prism has nothing to put back).
Command line: `prism allege on|off <key>` with keys such as `svc:sysmain`,
`app:msteams`, `task:\microsoft\windows\…` or `pol:hklm\…\value`. A service set back to
automatic is started again at once (Prism waits for a stop still in progress).

**Extreme level** (*Extrême* in the app, *Allègement* page):

- ~28 background services (diagnostics, Windows AI fabric, network discovery,
  telephony, payments/NFC, Edge updaters, biometrics…) are set to **start on demand**
  and stopped. *On demand* is not *disabled*: Windows starts the service again by itself
  the moment a program asks for it, so nothing breaks.
- Copilot, Teams chat button and Cortana are turned off by policy.
- 22 preinstalled apps are **removed for your account**: Widgets (its WebView host keeps
  150–400 MB in memory), Teams, new Outlook, Phone Link, Copilot, Clipchamp, To Do,
  News, Weather, Solitaire… The Store, Xbox/Game Pass, runtimes, security, the shell,
  Photos, Notepad, Calculator, Terminal and Paint are protected and can never be removed.
- **Undo**: `prism allege restore` (or *Tout restaurer*). Removed apps are re-registered
  from the copy Windows keeps on disk — offline, no Microsoft account, ~20 s for all
  of them (measured in the VM); the Microsoft Store is only a fallback.
- What you gain depends on what was running: on a clean Windows nothing measurable;
  on a PC where Teams, Outlook, Phone Link and Widgets run in the background, several
  hundred MB.

**Kernel anti-cheat games** (*Jeux à anti-cheat noyau*, top of the *Allègement* page;
`prism jeu-noyau [on|off]`): automatic, every item can be switched off. When Game Mode
starts and a kernel anti-cheat is running (Vanguard, EasyAntiCheat, BattlEye, FACEIT,
EA AntiCheat, Call of Duty's Ricochet), Prism:

1. puts the Extreme-level services back to their original settings for the game;
2. closes the tools anti-cheats refuse (debuggers, Cheat Engine, System Informer) —
   optionally every cybersecurity tool, virtual machines included (off by default:
   a closed VM loses its unsaved work);
3. stops the tools' services and drivers (Npcap, VMware, Sysinternals drivers) and
   shuts WSL down (gives back its virtual machine's memory);
4. at the end of the game, sets the Extreme services back to on-demand and restarts the
   stopped services. Closed programs are not reopened.

**Background WebViews** (*WebView en arrière-plan*, *Allègement* page; `prism webview`
lists them per app with their memory, `prism webview on|off`): an app sitting in the
notification area with no window often keeps a whole WebView2 engine in memory
(`msedgewebview2.exe`, 100–600 MB). Prism closes it once the app has had no visible
window for the chosen delay (10 min by default; a minimized window counts as visible).
The app itself keeps running and rebuilds its view when you open it again. Never for a
game, an anti-cheat, a game companion, a protected process or an app you exclude.

Some apps rebuild their WebView at once, or restart to do it (measured: Teams restarts
with a new 570 MB WebView). Closing it again would only fight the app, so Prism closes an
app's WebView at most once until you reopen the app, and an app that rebuilds it within
5 minutes is added to the exclusions for good, with a message: for those, the only real
saving is to quit the app or remove it from startup.

A tool can be marked *never touched*. After a crash, the end-of-game step is replayed
when Prism starts again. Prism never uninstalls a tool for a game (reinstalling would take
minutes every time); uninstalling is a separate action.

## 8a. Services (`prism services`, administrator)

The *Services* page lists every Windows service (running or not, start mode) with a
search box and a *running only* filter. Each one can be set to *Auto*, *Auto (différé)*,
*Manuel* or *Désactivé*:

- *Manuel* (on demand) is the safe way to slim Windows down: Windows starts the service
  by itself the moment a program needs it. *Désactivé* means never;
- the services anti-cheats, updates and security depend on (the 72 protected ones) are
  locked, with the reason shown; per-user service instances and kernel services are not
  set here;
- every change is journaled like debloat: the ↺ button puts that service back, and
  *Tout restaurer* (Allègement page) puts back all of them.

Quick actions: a switch per service (off = disabled and stopped; on = the original
setting back, or on demand if it was already disabled before Prism), a *Superflus
seulement* filter (services Prism considers useless: its catalogue plus third-party
updaters such as Logitech G Hub, Adobe, Google, Brave, Mozilla, Office Click-to-Run), and
*Désactiver tous les superflus* (protected ones excluded, reversible). The list loads in
the background and only visible rows are drawn.

```powershell
prism services                      # all services, start mode, 🔒 for protected ones
prism services Spooler manuel       # auto | differe | manuel | desactive | origine
```

## 8b. Privacy (`prism vie-privee`, administrator)

```powershell
prism vie-privee                  # dashboard: what is in place, and telemetry talking right now
prism vie-privee apply            # "recommande" (recommended): nothing useful is lost
prism vie-privee apply strict     # recommended + settings where you give something up (each one says what)
prism vie-privee restore          # put everything back as it was
```

Or in the app: **Vie privée** page — a score (protections in place), the
*Recommandé* / *Strict* / *Tout restaurer* buttons, a live **Télémétrie en ce moment**
panel and one line per protection.

- **Recommended**: no diagnostic logs or full memory dumps sent, no feedback prompts,
  no "tailored experiences", no suggested content, app suggestions, tips or
  recommendations in Settings and Start, Start search stays on your PC (no Bing), app
  launches not tracked, typing and handwriting not collected, Edge diagnostic data and
  personalization off, and **Windows Firewall rules** that cut CompatTelRunner and
  DeviceCensus off from the internet.
- **The telemetry service itself (DiagTrack) can only be stopped by disabling it**:
  run `prism allege apply` (debloat, safe level) — the dashboard shows whether it is
  done. A firewall rule does not stop it (measured, see measurements §7).
- **Strict** adds: error reports not sent, no online speech recognition / input
  personalization, Windows Copilot off, location off, no cloud clipboard, apps can't
  read other apps' diagnostic info. The app shows "On renonce à…" (what you give up)
  under each of these.
- Read-only lines show what **debloat** (`prism allege`, safe level) already does:
  telemetry service and tasks, advertising ID, activity history, Recall. Apply
  `prism allege apply` too for the full picture.
- Some settings apply at your next sign-in (marked *à la reconnexion*).
- Firewall rules are named `Prism OS - …` (visible in *Windows Defender Firewall with
  Advanced Security*). Only Windows' own telemetry programs are ever blocked — never a
  game, never Windows Update, Defender or licensing.
- On Windows Home and Pro, Windows keeps the "required" diagnostic level whatever the
  policy says (Microsoft's rule); the firewall rules work on every edition.

**One item at a time**: each setting and firewall rule on the *Vie privée* page has its
own checkbox (tick to apply, untick to put the original back). The lines checked through
debloat (*via prism allege*) stay read-only there — two journals on the same value would
undo each other. Command line: `prism vie-privee` shows each line's key, `prism
vie-privee on|off <key>` (e.g. `rule:prism os - compattelrunner`).

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

- **tiling** (*Fenêtres en tuiles*): windows arrange themselves side by side on each
  screen — *Principale + pile* (main window + stack), *Colonnes*, *Spirale*, *Monocle* —
  with adjustable gaps and main width, and a list of programs never tiled. Shortcuts:
  **Win+Ctrl+Alt+W** on/off, **+Space** next layout, **+Enter** active window as main,
  **+Left/Right** previous/next window, **+Up/Down** main width, **+F** float/tile the
  active window. Drop a window on another one to swap them. Games, fullscreen windows,
  dialogs and maximized windows are never tiled; a window too large for its tile (some
  apps impose a minimum size) stays floating, and no tile gets smaller than 320 × 200:
  extra windows stay where they are. Turning tiling off puts every window back
  where it was;
- **theme** (top of the page): futuristic themes — Prism (deep navy and bright cyan,
  the default), Néon, Cyberpunk, Holo, Synthwave, Aurora, Carbone — and Nord, Dracula,
  Catppuccin Mocha, Gruvbox, Tokyo Night, Rouge gaming, Clair (light, Solarized); every
  theme keeps text at a contrast of at least 7:1 (tested). **Couleurs personnalisées**:
  pick every colour yourself — background, panels, cards, raised cards, borders, text,
  secondary text, accent, success, warning, error — on top of any theme, and go back to
  the theme's colours in one click. The bar, the desktop widgets and the app change
  together; text on the accent colour switches between dark and light to stay readable;
- **all of Windows in the theme's colours** (*Tout Windows aux couleurs du thème*):
  Windows' own accent colour (title bars, borders, selections, Windows' Start menu),
  a futuristic wallpaper generated in the theme's colours at your screen's size (glowing
  horizon, perspective grid, faint diamond), dark mode and coloured title bars — each
  optional, all official Windows settings. *Remettre Windows comme avant* puts back the
  previous accent, wallpaper and appearance settings (originals saved in
  `%LOCALAPPDATA%\Prism\design-windows.json`). Command line: `prism design
  appliquer|restaurer` (accent and wallpaper);

- position (top, bottom, left, right), thickness, margin (floating bar), opacity,
  rounded corners;
- **replace the Windows taskbar** while the bar runs: Windows' taskbars (every screen)
  are hidden completely — not in pop-up auto-hide — and shown again if the bar stops;
- hide during fullscreen games and videos — only the bar of the screen showing the
  game or video steps aside;
- **several screens**: *Une barre sur chaque écran* (a bar on every screen, default) and
  *Chaque barre montre les fenêtres de son écran* (each bar shows the windows of its own
  screen, default; off = every bar shows every window). Screens plugged in or unplugged
  are picked up within a second; a window moved to another screen moves to that
  screen's bar, and the genie effect follows it there;
- bar widgets: Start button, open windows (click to switch), CPU, RAM, GPU, network,
  Game Mode indicator, clock;
- **desktop widgets**: clock, system panel, CPU/RAM/GPU graphs, network speed. Drag them
  with the mouse; their position is remembered;
- **per-app transparency**: e.g. `windowsterminal.exe` at 90 %. Never applied to a game
  or a fullscreen window;
- **per-element transparency**: separate opacity for menus and context menus, dropdown
  lists, tooltips and dialog boxes, in every classic app (100 % = untouched). Menus drawn
  inside modern WinUI apps are part of the app window and cannot be targeted on their own.

Settings are stored in `%ProgramData%\Prism\bar.json` and applied within a second.

### Icons and hover labels

The bar shows icons rather than names — chip (processor), memory module, screen (GPU),
antenna (network), gamepad (Game Mode, lit during a game) — and hovering any element
shows its name and details in a small bubble: *Processeur : 12 %*, *Mémoire : 9.8 Go /
31.1 Go (32 %)*, a window's title, the full date on the clock, etc.

### Open windows

Each open window has a button with its app icon (and its title when there is room);
the active one is underlined. Buttons keep the order windows were opened in. Like the
Windows taskbar: click a window to bring it to the front, click the active one to
minimize it, middle-click to close it, right-click for *Restaurer / Réduire / Agrandir /
Fermer*. With more windows than fit, the most recently used ones are shown and a **+N**
button lists all the others (click one to bring it to the front).

Clicking the clock opens Windows' calendar and notifications. Right-clicking an empty
spot of the bar opens a menu: *Gestionnaire des tâches*, *Afficher le bureau*, *Menu
Démarrer de Windows*, *Réglages de la barre…* (opens Prism).

### Prism Start menu

The bar's Start button (and **Alt+F1**, like KDE) opens Prism's own Start menu: type to
search (accents ignored, initials work — `vsc` finds Visual Studio Code), arrows and
Enter to launch, Escape to clear then close, a click elsewhere closes it. Every app is
there, desktop and Store alike: the list is Windows' own Start list (`Get-StartApps`),
launched through `shell:AppsFolder` exactly like Windows does, refreshed in the
background every 10 minutes and cached for an instant first open. Right-click an app to
pin it at the top. Bottom row: open Prism, lock, sleep, restart, shut down.

No keyboard hook is installed (some anti-cheats flag them), so the Windows key keeps
opening Windows' menu. *Menu Démarrer de Prism* in the bar settings switches the Start
button back to Windows' menu.

### System tray (*Zone système* widget)

With the Windows taskbar hidden, its notification area (volume, network, app icons) is
out of reach, so the bar has a *Zone système* widget with three buttons:

- **volume** → Windows' Quick Settings (volume, Wi-Fi, Bluetooth, accessibility), like
  Win+A;
- **bell** → notifications and calendar, like Win+N;
- **arrow** → shows the Windows taskbar for 10 seconds, to reach the icons of apps in
  the notification area (Discord, Steam…), then hides it again.

Prism cannot host the notification area itself (Windows keeps it for Explorer while it
runs). The widget is added once, before the clock, to bars configured before it existed;
remove it in the widget list if you do not want it.

### Prism icon in the notification area

While the Prism Bar runs, Prism has its own icon in the Windows notification area. On
Windows 11 a new icon goes to the **hidden icons** (the arrow next to the clock, or the
arrow of the *Zone système* widget); drag it onto the taskbar to keep it in view.

- **click** → opens Prism (the window already open, if any);
- **middle click** → frees RAM now, and a notification shows the result ("Memory in use:
  9.9 → 7.4 GB (−2.5 GB)");
- **right click** → menu: current mode (Daily or Game), *Open Prism*, *Free RAM now*,
  *Quit the Prism Bar*.

Hovering the icon shows the current mode and memory use. Untick *Icône Prism dans la zone
de notification* (Appearance › Prism Bar › Options) to remove it.

### Window effects

Beyond Windows' own animations, the Prism Bar draws its own window effects. Turn them on
in **prism-ui → Apparence → Effets de fenêtres** (window effects):

| Setting | Choices |
|---|---|
| Réduire / Restaurer / Ouvrir / Fermer (minimize / restore / open / close) — one effect per action | Aucun (none) · Lampe de génie (genie lamp: sucked into its bar button, and back out) · Écrasement (squash: shrinks straight into its bar button) · Gélatine (jelly: spring-damped wobble) · Zoom et fondu (zoom and fade) · Fondu (fade) · Bascule 3D (3D tilt: falls back in perspective while fading) · Éclatement (fall apart: breaks into tumbling pieces; reassembles when opening) |
| Agrandir / ancrer (maximize, snap to a screen edge, back to normal size) | Aucun · Glisse (glide: the window slides and stretches from its old frame to the new one) · Gélatine (same path with an elastic bounce) · Fondu enchaîné (crossfade in place) |
| Durée (duration) | 120–900 ms |
| Intensité (deformation intensity) | 0–100 % |
| Gélatine pendant le déplacement (jelly while dragging) | on (default) / off |

- The Prism Bar must be running: it plays the effects in a click-through overlay.
- Maximize/snap animations play for the maximize button, double-clicking the title
  bar, Win+arrow keys, dropping a window on a screen edge (straight from the drag jelly)
  and any app that resizes its own window. They start from the window's last image and
  fade into the real window at its new size.
- Turning the effects on also sets Windows' own minimize animation to *Instantanée*
  (instant), so the two do not overlap. **Apparence → Réglages d'origine** (original
  settings) puts it back.
- Effects never apply to a game, to a fullscreen window, or while Game Mode is active.
- Minimize and restore get the effect whether you click the bar button, use the window's
  title-bar button or a keyboard shortcut.
- **Jelly while dragging**: grab a window by its title bar and move it. The point you
  hold stays under the cursor, the rest of the window trails behind, overshoots when you
  stop and settles. The intensity slider also sets how soft it is. During the drag the
  window shows the image it had when you grabbed it (a playing video freezes until you
  release). Resizing, snapping to a screen edge or dragging a maximized window gives the
  window straight back to Windows.

Check them and their cost on your PC:

```powershell
prism fx demo     # plays minimize, restore, close and open on the foreground window (nothing is really closed)
prism fx stats    # latency, time per frame and dropped frames of the last 50 animations
```

A smooth effect shows `0/…` dropped frames and a latency of a few milliseconds. Measured
figures: [measurements §6](measurements.md).

## 11. Undo everything

| What | Command |
|---|---|
| Game Mode / Daily Mode | stop `prism watch` (Ctrl-C) — restored automatically, also after a crash |
| Startup apps | `prism demarrage restore` |
| Debloat | `prism allege restore` |
| Privacy | `prism vie-privee restore` |
| Appearance | `prism apparence restore` |
| **Everything at once** | `prism desinstaller` (also run by the uninstaller) |
| Prism Bar, transparency, window effects | `prism bar off` (effects); `prism apparence restore` (Windows' minimize animation) |

Journals live in `%ProgramData%\Prism\` (`journal.json`, `quotidien.json`,
`demarrage.json`, `allegement.json`, `apparence.json`, `vie-privee.json`).

## 12. Games and the console mode

```powershell
prism jeux                       # installed games (Steam, Epic, GOG, Battle.net)
prism jeux lancer elden ring     # launch through its store
```

In prism-ui, **Jeux → Mode console** opens a fullscreen launcher you can drive with
the arrow keys and Enter (Esc to leave).

**Per-game settings** (*Réglages par jeu*, under the game tiles): for each installed game,
two Windows settings on its executable — the same ones as *Settings > Display >
Graphics* and *Properties > Compatibility*, nothing is written into the game, compatible
with anti-cheats:

- *Carte graphique haute performance*: Windows runs the game on the dedicated GPU
  (laptops and PCs with integrated + dedicated graphics);
- *Plein écran exclusif*: turns off Windows' fullscreen optimizations for that game
  (some games get lower input latency or fewer stutters; others prefer them on — try it).

Prism finds the game's executable in its folder (the largest one; uninstallers, crash
reporters and redistributables are skipped). Takes effect at the next launch; unticking
puts the original value back exactly (other compatibility flags you set are kept), and
uninstalling Prism puts everything back. Command line: `prism jeux gpu|plein-ecran
<exe> on|off`.

**Last game** (*Dernière partie*): game, start, duration, how many Game Mode settings
were applied and put back, available memory at the start and at its lowest, and whether
the kernel anti-cheat plan ran.

## 13. Cybersecurity tools

```powershell
prism tools                  # packs: reseau (network), web, reverse, kali
prism tools reverse          # what a pack installs, with the exact commands
prism tools install kali     # Kali Linux under WSL + essential tools (nmap, sqlmap, hashcat…)
prism tools uninstall x64dbg # uninstall a tool or a whole pack
```

In the app (*Outils cyber*), every tool shows whether it is installed and has its own
*Installer* / *Désinstaller* button, plus *Tout installer* / *Tout désinstaller* per
pack. Installing and uninstalling run in the background without any console window
(winget silent mode), with *Installation en cours : Wireshark (2/3)* and a summary of
successes and failures; you can keep using Prism meanwhile. A tool installs its
requirements first (Kali before the Kali tools). Uninstalling Kali Linux erases the WSL
distribution and every file in it, so the app asks for confirmation — to free its memory
during a game this is not needed, Prism shuts WSL down by itself.

Nothing is installed by default and nothing runs while you play (the gaming profile
stops WSL when a game starts). Tools that can upset anti-cheats (debuggers, kernel-driver
tools) are flagged, and `prism status` warns if one is open. With a kernel anti-cheat
game, they are put on standby automatically — see *Kernel anti-cheat games* in §8.
Cheat Engine, System Informer / Process Hacker, VMware and VirtualBox are recognised
for standby even though Prism does not install them.

## 14. Configuration

```powershell
prism config init     # writes an editable copy to %ProgramData%\Prism\config.toml
prism config check    # validates it
prism config path
```

Invalid files are refused with a clear message rather than half-applied.

### Export and import your whole setup

```powershell
prism config export my-setup.json   # bar and theme, Prism configuration, appearance, applied levels
prism config import my-setup.json   # on this PC or another one (administrator for the levels)
```

The file holds the Prism Bar settings (edge, widgets, transparency, effects, theme…),
your `config.toml` if you have one, the Windows appearance settings, and which debloat
and privacy levels are **fully** applied. Importing writes the bar settings (the bar
picks them up within a second), keeps your previous `config.toml` as
`config.toml.bak`, then applies appearance and levels through the usual journals — so
`prism apparence restore`, `prism allege restore` and `prism vie-privee restore` still
undo everything. Import only adds: a level absent from the file is not removed.
Journals are never exported (they describe the original state of *this* PC).

## 15. Troubleshooting

| Symptom | Cause / fix |
|---|---|
| The engine does not start with Windows | `prism autostart on` in an administrator terminal (recreates both scheduled tasks) |
| "droits administrateur requis" (administrator rights required) | Open the terminal as administrator, or use "Relancer en administrateur" in prism-ui |
| Widgets setting refused by Windows | Recent Windows 11 builds lock some settings; Prism does not bypass Windows protections |
| GPU widget shows "—" | No GPU performance counters available on this machine |
| A game refuses to start because of a tool | Close the flagged debugger/kernel-driver tool shown by `prism status` |
| A window stays invisible after the Prism Bar was killed during an effect | Start the bar again (`prism bar on`): it makes such windows visible again (journal `fx-hidden.json`) |
| An effect does not play | Create an empty `%ProgramData%\Prism\fx-debug.log`: within a second the bar writes what it receives and decides there (delete the file to stop) |

- **The app uses CPU while open**: only the dashboard (every 2 s) and the privacy page
  (every 3 s) redraw by themselves; on a PC without a working graphics driver (virtual
  machines, basic display adapter) each frame is computed by the processor and costs a
  lot — minimize or close the app, the engine and the bar do not need it. To see what
  makes it redraw, start it with the environment variable `PRISM_UI_DEBUG=1`: frames per
  second and their causes go to `%LOCALAPPDATA%\Prism\ui-debug.log`.
