# Safety model

The launcher's promise is that every Mod Organizer 2 installation stays exactly as its owner left it. This page lists everything the launcher touches.

## Read, never written

| What | Why |
| --- | --- |
| `ModOrganizer.exe` (existence, icon) | Validate the entry; card icon. |
| `ModOrganizer.ini` | Game name and path, selected profile, configured executables, profile folder. |
| `portable.txt`, `*.compiler_settings` (names only) | Portable detection; Wabbajack modlist name. |
| `profiles\*` (folder names only) | Profile picker. |
| Game executables named by MO2 (existence, icon) | Play validation; card icon. |
| Steam `appcache\librarycache\<appid>\*` | Card artwork, displayed only, never copied. |
| Steam `steamapps\*.acf`, `libraryfolders.vdf`, `config\loginusers.vdf` | Match an MO2 game folder to a Steam app; account names. |

Enforcement:

- MO2 files are opened with read-only calls (`std::fs::read`, `read_dir`, metadata).
- The core crate has no code path that writes outside the launcher data folder. All launcher writes go through `DataDir`, which rejects absolute paths and `..` (`crates/core/src/config.rs`).
- Tests snapshot fake installations before and after reading and scanning, and fail on any change (`reading_never_modifies_the_install`, `scanning_does_not_modify_installs`).

## Written by the launcher

| Where | When |
| --- | --- |
| `%LOCALAPPDATA%\Modlist Launcher\` | Configuration, custom artwork copies, Steam backups, log. |
| `Desktop\*.lnk`, `Start Menu\Programs\*.lnk` | Only when you choose Add to Desktop or Add to Start Menu. An existing file is only replaced after you confirm. |
| One account's Steam `userdata\<id>\config\localconfig.vdf` | Only when you press Apply or Restore in the Steam Play Button sheet. |

When it changes Steam's `localconfig.vdf`, the launcher:

- Refuses while Steam is running, because Steam rewrites the file when it exits.
- Saves a full copy to `steam-backups\` first.
- Changes the bytes of exactly one value (that game's `LaunchOptions`). Every other byte of the file stays identical, and tests assert this.
- Writes a temporary file and renames it over the original.
- Re-reads the file and puts the original back if the value doesn't match.
- Records the previous value so **Restore Previous** can put it back. If someone changed the value since, Restore leaves it alone.

## Processes

- The launcher starts only the configured `ModOrganizer.exe`. It passes either nothing, `-p <profile>`, or `moshortcut://:<executable title>`, which MO2 itself defines.
- It never passes MO2's unsupported `--multiple` flag.
- Steam mode only starts Steam's original `%command%`, and only when you choose Without Mods.
- Process detection uses limited query rights (`PROCESS_QUERY_LIMITED_INFORMATION`). No administrator rights are needed.

## Not done, by design

- No network access. CI's dependency policy bans HTTP and TLS crates.
- No telemetry, accounts or update checks.
- No changes to MO2 settings, profiles, mods, downloads, overwrite or Wabbajack files.
- No closing or killing of other programs.
