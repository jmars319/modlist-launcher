# ADR 0001: Rust and Slint, with Slint's software renderer

Date: 2026-10-17
Status: accepted. The look (Cupertino, close to macOS) was replaced by the Vaexil theme in [ADR 0002](0002-vaexil-theme.md).

## Context

The launcher must start fast, use little memory while it waits in the background for a Steam session, and follow Windows dark mode and accessibility conventions. It should also look modern, close to macOS app design, and avoid web runtimes. The machine already had the Rust and MSVC toolchain and no .NET SDK.

Options considered:

| Option | Why not chosen |
| --- | --- |
| Raw Win32 | Smallest result, but common-control dark mode is mostly undocumented, and cards, focus rings and screen-reader support would all be hand-built. |
| WPF on .NET | Most mature screen-reader support, but needs either the .NET runtime or a 70 MB+ self-contained exe, and more memory while idle. |
| WinUI 3 | Needs the Windows App SDK runtime, starts slower, and is harder to package unpackaged. |
| Tauri / Electron | Web runtimes, which were explicitly not wanted. |

## Decision

- **UI:** Rust with Slint 1.18 and its `cupertino` style. The style follows the system light/dark setting, and its standard widgets expose UI Automation through AccessKit.
- **Windows APIs:** called through Microsoft's `windows` crate for processes, shortcuts, icons, file dialogs, clipboard and the registry.
- **Rendering:** Slint's software renderer only. Measured on the same release build and configuration:

| Renderer | Working set | Private | Warm start |
| --- | --- | --- | --- |
| OpenGL (femtovg) | ~111 MB | ~147 MB | ~25 ms |
| Software | ~34 MB | ~16 MB | ~30 ms |

The software renderer does not clip images to rounded corners, so cards draw small corner caps over the artwork instead.

## Consequences

- Single exe (~10 MB) with no runtime to install, about 30 ms warm start, and a small background footprint during Steam sessions.
- Narrator reads cards as named groups such as "Project Ultrapunk, Cyberpunk 2077, Running", with labeled buttons. Screen-reader maturity is below WPF's; changes to the UI should be re-checked with UI Automation.
- Slint is used under GPL-3.0, which matches this project's license.
