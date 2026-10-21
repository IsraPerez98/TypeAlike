# Architecture

- Virtual workspace, flat `crates/`. Never nest crates in `src/`.
- Deps point inward: `ghosttype` → `ghosttype_windows` / `ghosttype_macos` / `ghosttype_models` / `ghosttype_llm` / `ghosttype_history` / `ghosttype_settings` → `ghosttype_core`. Core never depends outward. Platform crates get services like `ModelHub` and `ModelRunner` as core trait objects from the bin, never by depending on `ghosttype_models`, `ghosttype_llm`, `ghosttype_history` or `ghosttype_settings`. The bin gives the platform `KeyVault` to `ghosttype_history` the same way.
- `ghosttype_core`: pure Rust. No OS API, no `windows` crate, no `unsafe`, no IO, no threads/clock (inject). Full unit tests.
- `ghosttype_windows`, `ghosttype_macos`: all `unsafe`/FFI, behind safe wrappers. Implements core traits. Minimal logic.
- `ghosttype`: thin bin. Wiring, config load, logging setup, event loop. No business logic.
- Core defines traits (text source, suggestion provider, injector, clock); platform implements. OS/IO behind traits → core tests run headless.
- New feature (AI provider, settings UI, other OS) = new module/crate behind core trait. See `portability.md`.
- New crate only for clear boundary, own deps, or compile time; name `ghosttype_<area>`.
- No globals/statics; state owned and passed explicitly.
