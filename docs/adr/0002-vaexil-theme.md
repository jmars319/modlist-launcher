# ADR 0002: Vaexil theme instead of the Cupertino look

Date: 2026-10-24
Status: accepted. It replaces the "close to macOS app design" goal in ADR 0001; the rest of 0001 stands.

## Context

The owner wants Modlist Launcher and the planned Vaexil SMF app (same Rust + Slint structure) to look clearly JAMARQ/Vaexil and to match each other. vaexil.tv defines the look:

- A `#05070d` canvas.
- Slate text.
- A cyan accent: `#22d3ee` and `#67e8f9`.
- Hairline `white/10` borders.
- Pill buttons and status chips.
- Geist type.

vaexil.tv is dark-only, while the launcher follows the Windows light/dark setting.

Slint's `cupertino` style cannot carry this on its own. Its `Palette` is read-only from `.slint` code, and its accent is tinted from the Windows accent colour, so standard buttons, switches, checkboxes and drop-downs would keep a system-coloured accent.

## Decision

- `crates/app/ui/theme.slint` holds the whole theme in one file that another Slint app can copy unchanged. It contains:
  - the `Theme` global (colours, radii, fonts);
  - Vaexil versions of `Button`, `ComboBox`, `Switch` and `CheckBox`;
  - the existing sheet, label and action-row components.
- **Dark palette:** taken from vaexil.tv.
- **Light palette:** its own palette, not an inversion of dark. A deeper cyan (`#0e7490`) keeps white button text at 5.4:1. Every text colour meets WCAG AA (4.5:1) on both the window and card backgrounds. The focus ring is 3:1 or better in both themes.
- **Status colours:** emerald for ready, violet for running, amber for warning and rose for danger, as on the site.
- **Remaining Cupertino widgets:** `LineEdit` and `ScrollView` still come from `cupertino`. They have little accent, and their focus ring follows the Windows accent colour.
- **App icon:** keeps its stacked-cards-and-play symbol, redrawn on the Vaexil canvas with the mark's blue-to-magenta. The Vaexil "V" itself stays the brand mark rather than this app's icon, so Vaexil apps stay distinguishable in the taskbar.
- **Fonts:** Geist and Geist Mono (OFL-1.1), embedded in the exe.
  - The source is the official vercel/geist-font v1.8.2 release (fonts version 1.802).
  - Only the static TTFs the UI uses are included: Geist Regular, SemiBold and Bold, plus Geist Mono Regular. They are in `assets/fonts/` with `OFL.txt`.
  - `app.slint` imports them, and every window sets `default-font-family: Theme.font-sans`.
  - The exe grows by about 575 KB; idle memory is unchanged.
  - The OFL text ships in `THIRD-PARTY-LICENSES.txt` in the zip and the installer.

## Consequences

- Accessibility is unchanged:
  - The custom widgets set the same UI Automation roles, names, checked state and default actions as the ones they replace.
  - Keyboard behaviour is unchanged: Space/Enter activate; Up/Down change a drop-down; a disabled button cannot take focus.
  - `scripts/smoke-arrow-keys.ps1` and `scripts/smoke-minimize-restore.ps1` pass.
- `Theme.window-background-alt` must stay one step off `window-background`, because the repaint-after-restore workaround depends on it.
- A shared theme crate is not worth it yet. Slint has no package mechanism for `.slint` files short of a Rust crate with an include path. Copying one file is simpler until a third app needs it.
