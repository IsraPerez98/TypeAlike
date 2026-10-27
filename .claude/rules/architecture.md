# Architecture

- Virtual workspace, flat `crates/`. Never nest crates in `src/`.
- Deps point inward: `typealike` → `typealike_windows` / `typealike_macos` / `typealike_models` / `typealike_llm` / `typealike_history` / `typealike_settings` → `typealike_core`. Core never depends outward. Platform crates get services like `ModelHub` and `ModelRunner` as core trait objects from the bin, never by depending on `typealike_models`, `typealike_llm`, `typealike_history` or `typealike_settings`. The bin gives the platform `KeyVault` to `typealike_history` the same way.
- `typealike_core`: pure Rust. No OS API, no `windows` crate, no `unsafe`, no IO, no threads/clock (inject). Full unit tests.
- `typealike_windows`, `typealike_macos`: all `unsafe`/FFI, behind safe wrappers. Implements core traits. Minimal logic.
- `typealike`: thin bin. Wiring, data folders, settings load, logging, event loop. No business logic.
- Core defines traits (text source, suggestion provider, injector, clock); platform implements. OS/IO behind traits → core tests run headless.
- New feature (AI provider, settings UI, other OS) = new module/crate behind core trait. See `portability.md`.
- New crate only for clear boundary, own deps, or compile time; name `typealike_<area>`.
- No globals/statics; state owned and passed explicitly.
