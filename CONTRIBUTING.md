# Contributing

Bug reports and pull requests are welcome. Modlist Launcher is small on purpose: one job, done safely, with no network access.

## Before you start

- For anything bigger than a fix, open an issue first so we can agree on the approach.
- Read the [safety model](docs/safety-model.md). Changes must keep its promises:
  - never write to an MO2 installation;
  - change Steam settings only on an explicit action, with Steam closed and after a backup;
  - make no network connections;
  - need no admin rights.
- Design decisions are recorded in [docs/adr](docs/adr). The look comes from the Vaexil theme in `crates/app/ui/theme.slint` (ADR 0002). Use its components and `Theme` tokens rather than new colours.

## Working on it

You need the Rust toolchain (stable, MSVC) and Visual Studio Build Tools. Set `MODLIST_LAUNCHER_DATA` to a scratch folder so development runs never touch your real configuration.

```powershell
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release -p modlist-launcher
./scripts/smoke-minimize-restore.ps1
./scripts/smoke-arrow-keys.ps1
```

CI runs all of these on every pull request, plus `cargo deny check`. `cargo deny` bans HTTP and TLS crates and rejects yanked or vulnerable ones. Both CI checks must pass before a pull request can merge.

- Keep each pull request to one change.
- Add tests for behaviour that can be tested. Platform-neutral logic belongs in `crates/core`, where it is unit tested.
- For UI changes:
  - check light and dark mode;
  - check keyboard use: Tab, arrow keys between cards, Space/Enter;
  - check that controls have screen-reader labels.
- Update the README or docs when behaviour changes.

## License

By contributing you agree that your work is licensed under [GPL-3.0-only](LICENSE), like the rest of the project.
