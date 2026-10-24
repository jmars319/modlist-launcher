# Security

Modlist Launcher runs with your normal user rights. It never asks for admin rights, and it makes no network connections. The [safety model](docs/safety-model.md) lists every file it reads and writes. In short, it never writes to an MO2 installation. It writes only:

- its own data folder;
- shortcuts you ask for;
- one launch-options value in Steam's settings, when you press Apply or Restore, with Steam closed and after a backup.

## Reporting a problem

Report anything that could harm a user's files, Steam settings or PC privately, through GitHub: **Security > Report a vulnerability** on this repository. Please do not open a public issue for these.

Examples worth reporting:

- the launcher writing anywhere the safety model says it does not, especially inside an MO2 installation;
- Steam launch options changed without Apply, Restore, Remove or uninstall, or not restored when they should be;
- a crafted `ModOrganizer.ini`, shortcut or Steam file that makes the launcher run something other than the configured `ModOrganizer.exe` or its executables.

Include the launcher version (Settings shows it), what you did, and what happened. `launcher.log` in the data folder helps. Check it for anything private before attaching it.

You should get a reply within a week. Fixes go into the next release, and the report is credited unless you would rather it was not.

## Supported versions

Only the latest release gets fixes.
