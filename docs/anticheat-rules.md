# Anti-cheat compatibility rules (non-negotiable)

To an anti-cheat, Prism must remain **an ordinary Windows**. Any contribution that
breaks one of these rules is rejected.

1. **Never touch the game process**: no injection, no reading or writing of its memory,
   no handle opened on it beyond reading its name and creation time.
2. **Never touch anti-cheat processes** (`vgc`, `vgtray`, `EasyAntiCheat*`,
   `BEService*`, `FACEIT*`, `EAAntiCheat*`…), antivirus processes, or system processes.
   The list lives in `config/default.toml` (`protected`).
3. **No Prism kernel driver.** Everything goes through documented user-mode APIs.
4. **Never disable a protection required by an anti-cheat**: Secure Boot, TPM,
   VBS/HVCI, Defender, driver signature enforcement, test mode off. If a "performance"
   option ever touches VBS, it will be explicit, come with a warning, and be reversible,
   never on by default.
5. **Never modify a signed system file.** Settings go through the official APIs,
   policies, and configuration.
6. **Windows Update stays functional**: anti-cheats require up-to-date versions.
7. **Report, don't hide**: a problematic tool (debugger, third-party kernel driver) is
   reported to the user, never hidden from the anti-cheat.
8. **Never touch the services session (session 0).** Prism only acts on processes of
   the interactive user session, even if Prism itself runs elsewhere (over SSH, or as a
   service): WMI providers and licensing services must stay untouched.
9. **Never touch a game's windows.** Per-app transparency, window styles and any future
   window management skip every process classified as a game (same rules as Game Mode,
   including detected library folders) and every fullscreen window.
