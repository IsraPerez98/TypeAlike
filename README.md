# GhostType

Open-source, system-wide inline autocomplete for Windows, inspired by [Cotypist](https://cotypist.app/).
Grey "ghost text" suggestions appear at the caret in any app; press **Tab** to accept.

> Status: early scaffold.

## Build

Requires Windows, the MSVC build tools, and [rustup](https://rustup.rs/) (the toolchain version is pinned
in `rust-toolchain.toml` and installed automatically).

```powershell
cargo run                              # debug build of the app
cargo build --release                  # target\release\GhostType.exe
cargo test --workspace                 # unit tests
cargo clippy --workspace --all-targets # lints
```

## Layout

| Crate | Purpose |
|---|---|
| `crates/ghosttype` | The `GhostType.exe` binary: wiring, configuration, logging |
| `crates/ghosttype_core` | Platform-independent suggestion and acceptance logic |
| `crates/ghosttype_windows` | Win32 / UI Automation integration: hooks, caret reading, overlay, input |

## Contributing

- **Test-driven:** write a failing test first, then the code that makes it pass.
- **Self-documenting code:** clear names and small functions instead of comments; doc comments on public items.
- **Checks:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets`, `cargo test --workspace` and
  `cargo doc --workspace --no-deps` must pass with warnings treated as errors (same as CI).
- **Commits:** Conventional Commits on short-lived branches off `main`.

Full coding rules live in [`.claude/rules/`](.claude/rules/).

## License

Planned: MIT OR Apache-2.0 (license files not yet added).
