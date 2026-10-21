# GhostType

Open-source, system-wide inline autocomplete for Windows and macOS, inspired by [Cotypist](https://cotypist.app/).
Gray "ghost text" suggestions appear at the caret in any app; press **Tab** to accept.

> Status: early scaffold.

## Install

Download the files from [Releases](https://github.com/IsraPerez98/GhostType/releases).

**Windows 10 or 11 (x64):** run one of the two installers. They install GhostType for the current user only.

- `GhostType-X.Y.Z-windows-x64-vulkan-setup.exe` runs the models on the CPU, or on a GPU with Vulkan.
- `GhostType-X.Y.Z-windows-x64-cuda-setup.exe` also runs the models with CUDA on an NVIDIA GPU. It needs a recent
  NVIDIA driver, and it is much larger.

The installers do not have a signature yet, so SmartScreen can show a warning. Click **More info**, then
**Run anyway**. The uninstaller keeps your settings and models in `%LOCALAPPDATA%\GhostType`.

**macOS 15 or later (Apple silicon):** open the `.dmg` file and move GhostType to Applications. The app does not
have a Developer ID signature yet, so macOS stops the first start:

1. Open GhostType. macOS shows that it cannot make sure that the app is safe.
2. Open System Settings > Privacy & Security, then click **Open Anyway**.
3. Give GhostType Accessibility access. For the screen text, also give it Screen & System Audio Recording access.

macOS does not keep these permissions after an update, because each build has a different ad-hoc signature.
Remove GhostType from each list, then give the access again.

## Build

Requires [rustup](https://rustup.rs/) (the toolchain version is pinned in `rust-toolchain.toml` and installed
automatically) and [CMake](https://cmake.org/), plus the MSVC build tools and LLVM on Windows or the Xcode Command
Line Tools on macOS. CMake and Clang build llama.cpp, which runs the language models.

On macOS, GhostType needs Accessibility access (System Settings > Privacy & Security > Accessibility) to read
the caret and type accepted text. When started with `cargo run`, grant access to the terminal app.

```sh
cargo run                              # debug build of the app
cargo build --release                  # optimized build in target/release
cargo test --workspace                 # unit tests
cargo clippy --workspace --all-targets # lints
```

## Layout

| Crate | Purpose |
|---|---|
| `crates/ghosttype` | The `GhostType` binary: wiring, data folders, logging |
| `crates/ghosttype_core` | Platform-independent logic: suggestions, acceptance, key rules, settings model and settings panes |
| `crates/ghosttype_windows` | Win32 / UI Automation integration: hooks, caret reading, overlay, caret badge, input, tray, settings window |
| `crates/ghosttype_macos` | AppKit / Accessibility integration: event tap, caret reading, overlay, input, menu bar, settings window, launch agent |
| `crates/ghosttype_models` | Hugging Face search, model downloads and the local models directory |
| `crates/ghosttype_llm` | Runs GGUF language models with llama.cpp |
| `crates/ghosttype_history` | The earlier writing of the user in one encrypted file |
| `crates/ghosttype_settings` | The settings file and its format |

## Contributing

- **Test-driven:** write a failing test first, then the code that makes it pass.
- **Self-documenting code:** clear names and small functions instead of comments; doc comments on public items.
- **Checks:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets`, `cargo test --workspace` and
  `cargo doc --workspace --no-deps` must pass with warnings treated as errors (same as CI).
- **Commits:** Conventional Commits on short-lived branches off `main`.

Full coding rules live in [`.claude/rules/`](.claude/rules/).

## License

Planned: MIT OR Apache-2.0 (license files not yet added).
