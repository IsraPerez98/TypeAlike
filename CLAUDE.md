# GhostType

OSS Windows-first Cotypist clone (https://cotypist.app/). System-wide inline autocomplete: ghost text at caret in any app, Tab accepts. Rust.

## Build
- Toolchain pinned `rust-toolchain.toml`; bump with `rust-version`.
- `cargo run` = app (default member `crates/ghosttype`, bin `GhostType`).
- Pre-commit = CI, all must pass: `cargo fmt --all --check`; `cargo clippy --workspace --all-targets` (`RUSTFLAGS=-D warnings`); `cargo test --workspace`; `cargo doc --workspace --no-deps`.

## Layout
Virtual workspace, flat `crates/`.
- `ghosttype_core`: engine, matching, accept state machine, config model, Models pane. Pure Rust, no OS.
- `ghosttype_windows`: hooks, UIA, overlay, `SendInput`, tray. All FFI.
- `ghosttype_macos`: event tap, Accessibility (AX), overlay panel, CGEvent injection, menu bar extra, settings window. All FFI. Needs Accessibility access (`cargo run` from a terminal: grant it to the terminal app).
- `ghosttype_models`: Hugging Face search, downloads and the local models directory (`ModelHub`). HTTPS + file IO, no FFI.
- `ghosttype`: bin. Wiring, config, logging, event loop.

## Rules
`.claude/rules/` (binding; path-scoped ones load with matching files):
- `tdd.md` test-first workflow · `clean-code.md` naming, structure, no comments · `architecture.md` crate boundaries, traits
- `portability.md` logic in core; platform crates only translate, so OS ports never touch core
- `rust.md` API, errors, docs, types · `dependencies.md` Cargo · `windows.md` FFI, threads, runtime invariants · `windows-ui.md` Windows 11 look & feel (Fluent) · `macos-ui.md` macOS look & feel (Apple HIG, Liquid Glass) · `git.md`
