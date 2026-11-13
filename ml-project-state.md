# Modlist Launcher project state

Plan artifact: https://claude.ai/artifact/2qaZ63YM974LMKSFwgbQZD ("The Modlist
Launcher Plan", the owner-facing page in five tabs; update it in place, never
publish a second one).

Working state across sessions. Durable detail lives in `README.md` and `docs/`
(`docs/safety-model.md`, `docs/steam-play-button.md`, `docs/adr/`); this file
holds what is true right now. Created 13 Nov 2026.

## Core Project Architecture & Stack

- Windows-only Rust + Slint 1.18 desktop app (software renderer), per-user
  Inno Setup installer plus portable zip. Workspace crates under `crates/`; UI
  in `crates/app/ui/*.slint`.
- `crates/app/ui/theme.slint` and `assets/fonts/` are shared byte-for-byte with
  Keeplight and Vaexil Mod Manager. Keep app-specific paths out of the theme;
  font imports live in `app.slint`. Agree any theme change with those sessions.
- Data: `%LOCALAPPDATA%\Modlist Launcher` (`config.json`, `launcher.log`,
  `steam-backups\`). Install: `%LOCALAPPDATA%\Programs\Modlist Launcher`.
- CI: `ci.yml` (tests, cargo-deny, three smoke scripts on the release build),
  `advisory-sweep.yml`, `release.yml` (a `v*` tag builds a draft release),
  `publish-feed.yml` (publishing the draft copies the installer to
  `JAMARQ-Digital-LLC/jamarq-releases`, secret `JAMARQ_RELEASES_TOKEN`).
- Verify: `cargo test --workspace --locked` (102 pass, 2 live tests ignored on
  13 Nov 2026), then `scripts/smoke-*.ps1` against a release build.

## Current Structural Milestones / Active Feature Goals

- Order set by the owner on 13 Nov 2026: stage 05 accessibility, then 06 a
  first GitHub Release (`v0.1.0`, unsigned, not announced), then 08 the
  "An update is available" notice.
- Stage 07 code signing is set aside (owner, 13 Nov 2026): the launcher is a
  tool for now and may be absorbed into Vaexil Mod Manager, or kept as a
  separate download. No public launch planned. Revisit signing (SignPath
  Foundation) only if it is offered on its own to other people.
- Stage 05 scope: live Narrator (owner listens), keyboard-only flows, Windows
  contrast themes, display scales 100-200% and mixed-DPI monitors, the
  off-screen card missing from the UIA tree. Leads: Slint's ScrollView already
  scrolls a focused control into view (Vaexil Mod Manager, 12 Nov), so the
  cards' `focused-card` tracking may only be needed for arrow keys.
- Stage 08 reads `modlist-launcher-latest/release.json` (url + sha256) from
  the shared feed: owner leaning yes on 13 Nov, default recorded on the plan.

## Active Task State & Known Regressions

- Installed exe is the 25 Oct build of `99595b6` (#21); main at `b510def`
  (#26) differs only in dependencies, a test fix and workflows.
- After the 8 Nov account move all four Steam Play hooks were re-applied and
  point at `C:\Users\Jason\...\modlist-launcher.exe`; Steam Play worked 4 times
  through 12 Nov.
- No releases or tags; release and feed workflows have never run.
- Watch: one copy left open four days kept running after its window closed
  (Oct); not reproduced.

## Temporary Logic Rules & Constraints

- No network access except the planned update check, which must use WinHTTP
  via the `windows` crate (deny.toml bans HTTP/TLS crates), be opt-out in
  Settings, download only on click, and be recorded in the decision log and
  `docs/safety-model.md`.
- No auto-update. New modlists are never added without the owner's OK.
- Steam's `localconfig.vdf` is written only while Steam is closed, always
  after a backup.
- `assets/icon.svg` and `scripts/generate-icon.ps1` change together; tell the
  Mac coordinating session the merge commit.
- pin-utils RUSTSEC-2026-0441 is ignored until 2027-02-08; fix together with
  Keeplight and Vaexil Mod Manager.

## Context Compaction Checklist

- Keep the plan artifact line first; the URL does not change.
- Record open PRs, the installed build, and any owner decision here.
- Update this file when a stage lands or a rule changes.
