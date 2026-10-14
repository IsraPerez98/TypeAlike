# Portability

Windows-first, not Windows-only. Porting to macOS/Linux = new `ghosttype_<os>` crate + bin wiring. Zero changes to `ghosttype_core`.

## Logic lives in core
- Every decision in `ghosttype_core`: what to suggest, when to show/hide/update ghost text, accept/reject, settings model + validation, tray menu items and their actions, app lifecycle state.
- Platform crate only translates: OS events → core inputs, core outputs → OS calls. Branches only for OS quirks; any other `if`/`match` belongs in core.
- UI split: core owns UI state (what is shown, enabled, checked, labelled); platform only renders it and reports user actions back. Same for overlay, tray, settings window.
- Test: if a behavior can't be unit-tested headless in core, it is in the wrong crate. Move it.

## OS-neutral core API
- Core types in domain terms, never OS terms: no `HWND`, virtual-key codes, UTF-16, Win32 message names, `\` paths, registry. Define own (`Key`, `ScreenRect` in physical px, `AppId`); platform converts at its boundary.
- OS services (text at caret, overlay, injection, autostart, config location, clock) = core traits, implemented per platform.
- Trait shaped by what core needs, not by what Win32 offers. Would a macOS impl fit it naturally? If not, reshape.

## Wiring
- Platform crates are target-gated deps of the bin (`[target.'cfg(windows)'.dependencies]`).
- Platform selection via `cfg(target_os)` in one bin module only; rest of the bin OS-agnostic.
- `ghosttype_core` builds and tests on Linux in CI. Keep it green.
