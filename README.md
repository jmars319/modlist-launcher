# Modlist Launcher

Plan artifact: https://claude.ai/artifact/2qaZ63YM974LMKSFwgbQZD

A small Windows launcher for multiple portable [Mod Organizer 2](https://github.com/ModOrganizer2/modorganizer) installations, such as Wabbajack modlists.

Pick a modlist and the launcher starts that installation's own `ModOrganizer.exe`, with the installation's folder as the working directory. Each installation stays completely independent. The launcher never copies, merges, moves or edits MO2 configuration, profiles, downloads, mods, overwrite folders or Wabbajack files.

Modlist Launcher is an independent project. It is not affiliated with the Mod Organizer 2 team or Wabbajack.

## Features

- **One card per modlist**, with artwork from Steam's local library cache, a custom image, or the game's own icon. Nothing is downloaded.
- **Play or Open MO2.**
  - **Play** runs one of the installation's configured MO2 executables directly (for example the game), using MO2's `moshortcut://` link.
  - **Open MO2** opens Mod Organizer's main window.
- **Profile picker** on each card. It passes `-p <profile>` to MO2, or leaves MO2's own choice alone.
- **Safe one-at-a-time launching.** Mod Organizer allows only one running instance on the whole PC.
  - Starting a second, different installation would hand its request to the one already open. The launcher blocks that and says which modlist to close first.
  - Pressing Open MO2 for a modlist that is already open brings its window forward.
- **Clear problems.** A missing exe, missing folder, removed MO2 executable or deleted profile is shown on the card, in plain language.
- **Add, edit, rename, reorder and remove** installations. Removing one only removes it from the launcher.
- **Scan folders** for portable MO2 installations. Scanning is read-only, and nothing is added until you tick the results and confirm.
- **New modlists noticed at startup.** When the window opens, the saved scan folders are scanned in the background. New portable installs appear in a notice with **Review** (opens the scan results, new ones ticked) and **Dismiss** (stops the notice for those installs). A notice never covers another message; it waits until that one is dismissed.
- **Shortcuts:**
  - Desktop and Start menu shortcuts for the launcher.
  - Per-modlist shortcuts that start a modlist directly (`--launch <id>`).
- **Setup at a glance.** Small chips under each card's game name show what is set up for it: **Steam Play** (Steam's Play button starts it), **Desktop** and **Start menu** (a shortcut to it in that folder, found by reading the `.lnk` files there).
- **Steam Play button** (optional): make Steam's own Play button start a modlist through MO2. Hold Shift while pressing Play to choose with or without mods. See [docs/steam-play-button.md](docs/steam-play-button.md).
- **Vaexil look:** the JAMARQ/Vaexil theme (dark canvas, cyan accent, pill buttons) in both light and dark ([ADR 0002](docs/adr/0002-vaexil-theme.md)).
- **Settings:** follows the Windows light/dark setting (or force either), high-DPI aware, keyboard and screen-reader friendly.
- **No telemetry, ads, accounts or network access. No administrator rights.**

## Install

Download from [Releases](https://github.com/jmars319/modlist-launcher/releases):

- `ModlistLauncher-<version>-setup.exe` is a per-user installer. It needs no administrator rights and installs to `%LOCALAPPDATA%\Programs\Modlist Launcher`.
- `ModlistLauncher-<version>-portable.zip` holds a single exe. Unzip it anywhere and run it.

Check downloads against `SHA256SUMS.txt`. Builds are not code-signed yet, so Windows SmartScreen may warn the first time.

## Use

1. **Add…** and choose a modlist's `ModOrganizer.exe`, or **Scan…** a folder such as `C:\Modlists`.
2. Check the card. **Play** starts the configured executable through MO2, and **Open MO2** opens Mod Organizer.
3. Under **•••**, you can edit the entry, connect it to Steam's Play button, create shortcuts, reorder it or remove it.

Keyboard:
- Tab and Shift+Tab move between controls, Enter or Space activates, and Esc closes a sheet.
- From any card, the arrow keys move to the neighbouring card, and Home/End go to the first or last. Focus lands on the card's main button. The profile list keeps Up/Down for itself.
- Ctrl+N adds an installation, and Ctrl+, opens Settings.

### Command line

```text
modlist-launcher                         Open the launcher
modlist-launcher --launch <id|name>      Run an installation's Play action
modlist-launcher --open <id|name>        Open an installation's Mod Organizer
modlist-launcher --steam <id> -- <cmd>   Used by Steam launch options (%command%)
```

## Where things are stored

The launcher's own files live in `%LOCALAPPDATA%\Modlist Launcher`. Nothing here is ever written inside an MO2 installation.

| File | What it is |
| --- | --- |
| `config.json` | Installations, order, settings. Saved atomically; the previous copy is kept as `config.json.bak`. A file that cannot be read is set aside, never silently overwritten. |
| `artwork/` | Copies of custom card images you chose. |
| `steam-backups/` | A copy of Steam's `localconfig.vdf`, taken before each Steam Play button change. |
| `launcher.log` | Launches, blocks and errors, for troubleshooting. Stays on this PC. |

See [docs/safety-model.md](docs/safety-model.md) for exactly what the launcher reads and writes.

## Build from source

You need the Rust toolchain (stable, MSVC) and Visual Studio Build Tools.

```powershell
cargo test --workspace
cargo run -p modlist-launcher
```

Set `MODLIST_LAUNCHER_DATA` to a scratch folder to keep development runs away from your real configuration.

To check that the window repaints after minimize/restore and maximize/restore, run this after a release build:

```powershell
./scripts/smoke-minimize-restore.ps1
```

It uses a throwaway config and fake installs, and screenshots the window after each restore. It exits 1 if a restored window differs from the first screenshot.

Packaging (needs Inno Setup 6 for the installer):

```powershell
./scripts/package.ps1
```

This builds in `dist/staging` and writes the zip, installer and `SHA256SUMS.txt` to `dist/`. It also verifies the zip contents. Tagging `v*` runs the same script in CI and creates a draft GitHub release.

### Layout

| Path | Purpose |
| --- | --- |
| `crates/core` | Platform-neutral logic, fully unit tested: config store, MO2 INI reader, discovery, validation, launch commands, single-instance gate, Steam VDF editing, CLI. |
| `crates/app` | The Windows app: Slint UI (`ui/`), window controller (`src/gui`), shortcut and Steam modes (`src/headless.rs`), and Windows APIs (`src/platform`). |
| `packaging/` | Inno Setup script. |
| `scripts/` | Packaging and icon generation. |
| `docs/` | Safety model, Steam setup, architecture decisions. |

## License

GPL-3.0-only. See [LICENSE](LICENSE). The embedded Geist and Geist Mono fonts are under the SIL Open Font License 1.1 ([assets/fonts/OFL.txt](assets/fonts/OFL.txt)). Mod Organizer 2 is also GPL-3.0. This launcher only starts it as a separate program and does not include any of its code.
