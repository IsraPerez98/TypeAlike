# GhostType

Open-source, system-wide inline autocomplete for Windows, inspired by [Cotypist](https://cotypist.app/).
Grey "ghost text" suggestions appear at the caret in any app; press **Tab** to accept.

> Status: early scaffold. The Windows plumbing was proven in a separate proof of concept and is being
> ported here.

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

## License

Planned: MIT OR Apache-2.0 (license files not yet added).
