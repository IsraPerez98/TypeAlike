---
paths:
  - "**/*.rs"
  - "**/Cargo.toml"
---
# Architecture

Windows-first, not Windows-only. A port to another OS is a new `typealike_<os>` crate and bin wiring, with zero changes to `typealike_core`.

## Crates

Virtual workspace, flat `crates/`. Never nest crates in `src/`.

- `typealike_core`: engine, matching, accept state machine, prompt, earlier writing, settings model, panes, setup and UI texts (`docs/core/`). Pure Rust. No OS API, no `windows` crate, no `unsafe`, no IO, no threads/clock (inject). Full unit tests.
- `typealike_windows`, `typealike_macos`: all `unsafe`/FFI, behind safe wrappers. Implements core traits. Minimal logic.
- `typealike_models`: Hugging Face search, downloads and the local models directory (`ModelHub`). HTTPS + file IO, no FFI.
- `typealike_history`: the earlier writing in one encrypted file (`WritingHistory`), with `ring` for the cipher and the random key. It reads the key from the `KeyVault` of the platform (`typealike_macos::Keychain`, `typealike_windows::ProtectedKeyFile`) only at the first read or write. File IO, no FFI, no network.
- `typealike_settings`: the settings file (`SettingsFile`, the `SettingsStore` of core): the serde format of the settings and the migrations of old fields (`stored`). File IO, no FFI.
- `typealike_llm`: runs GGUF models with llama.cpp (`docs/llm.md`).
- `typealike_releases`: the newest release of TypeAlike from the GitHub API (`GitHubReleases`, the `ReleaseFeed` of core): the check for updates of the About pane, and the download and the install of a new version. HTTPS and the processes of the installers (`hdiutil`, the Inno Setup installer), no FFI. Only its `install` module selects the OS.
- `typealike`: thin bin. Wiring, data folders, settings load, logging, event loop. No business logic.
- New crate only for a clear boundary, own deps, or compile time. Name it `typealike_<area>`.

## Dependencies point inward

- `typealike` → `typealike_windows` / `typealike_macos` / `typealike_models` / `typealike_llm` / `typealike_history` / `typealike_settings` / `typealike_releases` → `typealike_core`. Core never depends outward.
- Platform crates get services like `ModelHub` and `ModelRunner` as core trait objects from the bin, never by depending on `typealike_models`, `typealike_llm`, `typealike_history`, `typealike_settings` or `typealike_releases`. The bin gives the platform `KeyVault` to `typealike_history` the same way.
- Platform crates are target-gated deps of the bin (`[target.'cfg(windows)'.dependencies]`).
- Platform selection via `cfg(target_os)` in one bin module only. The rest of the bin is OS-agnostic.
- `typealike_core` builds and tests on Linux in CI. Keep it green.

## Logic lives in core

- Every decision in `typealike_core`: what to suggest, when to show/hide/update ghost text, accept/reject, settings model + validation, tray menu items and their actions, app lifecycle state.
- The `Controller` runs TypeAlike from the platform events (keys, clicks, timers, tray, settings window, service answers). Pure Rust, no OS.
- Each platform implements the `Shell` trait: it reads the desktop (`Desktop`) and does the OS effects that the controller asks for. The platform runtimes only translate OS events into controller calls.
- Platform crate only translates: OS events → core inputs, core outputs → OS calls. Branches only for OS quirks. Any other `if`/`match` belongs in core.
- UI split: core owns UI state (what is shown, enabled, checked, labeled). The platform only renders it and reports user actions back. Same for overlay, tray, settings window.
- Test: if a behavior cannot be unit-tested headless in core, it is in the wrong crate. Move it.
- New feature (AI provider, settings UI, other OS) = new module/crate behind core trait.
- No globals/statics. State is owned and passed explicitly.

## OS-neutral core API

- Core types in domain terms, never OS terms: no `HWND`, virtual-key codes, UTF-16, Win32 message names, `\` paths, registry. Define own types (`Key`, `ScreenRect` in physical px, `Permission`). The platform converts at its boundary.
- OS services and IO (text at caret, suggestion provider, overlay, injection, autostart, config location, clock) = core traits, implemented per platform, so core tests run headless.
- Trait shaped by what core needs, not by what Win32 offers. Would a macOS impl fit it naturally? If not, reshape.
