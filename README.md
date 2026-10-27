# TypeAlike

Open-source, system-wide inline autocomplete for Windows and macOS, inspired by [Cotypist](https://cotypist.app/).
"Ghost text" suggestions appear at the caret in any app. Press **Tab** to accept.

> Status: early scaffold.

## Install

Download the files from [Releases](https://github.com/IsraPerez98/TypeAlike/releases).

**Windows 10 or 11 (x64):** run one of the two installers. They install TypeAlike for the current user only.

- `TypeAlike-X.Y.Z-windows-x64-vulkan-setup.exe` runs the models on the CPU, or on a GPU with Vulkan.
- `TypeAlike-X.Y.Z-windows-x64-cuda-setup.exe` also runs the models with CUDA on an NVIDIA GPU. It needs a recent
  NVIDIA driver, and it is much larger.

The installers do not have a signature yet, so SmartScreen can show a warning. Click **More info**, then
**Run anyway**. The uninstaller keeps your settings and models in `%LOCALAPPDATA%\TypeAlike`.

**macOS 15 or later (Apple silicon):** open the `.dmg` file and move TypeAlike to Applications. The app does not
have a Developer ID signature yet, so macOS stops the first start:

1. Open TypeAlike. macOS shows that it cannot make sure that the app is safe.
2. Open System Settings > Privacy & Security, then click **Open Anyway**.
3. Give TypeAlike Accessibility access. For the screen text, also give it Screen & System Audio Recording access.

macOS does not keep these permissions after an update, because each build has a different ad-hoc signature.
Remove TypeAlike from each list, then give the access again.

## Build

Requires [rustup](https://rustup.rs/) (the toolchain version is pinned in `rust-toolchain.toml` and installed
automatically) and [CMake](https://cmake.org/), plus the MSVC build tools and LLVM on Windows or the Xcode Command
Line Tools on macOS. CMake and Clang build llama.cpp, which runs the language models.

On macOS, TypeAlike needs Accessibility access (System Settings > Privacy & Security > Accessibility) to read
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
| `crates/typealike` | The `TypeAlike` binary: wiring, data folders, settings load, logging, event loop |
| `crates/typealike_core` | Platform-independent logic: suggestions, acceptance, key rules, settings model and settings panes |
| `crates/typealike_windows` | Win32 / UI Automation integration: hooks, caret reading, overlay, caret badge, input, tray, settings window, screen text, startup entry |
| `crates/typealike_macos` | AppKit / Accessibility integration: event tap, caret reading, overlay, caret badge, input, menu bar, settings window, screen text, launch agent |
| `crates/typealike_models` | Hugging Face search, model downloads and the local models directory |
| `crates/typealike_llm` | Runs GGUF language models with llama.cpp |
| `crates/typealike_history` | The earlier writing of the user in one encrypted file |
| `crates/typealike_settings` | The settings file and its format |

## Contributing

- **Test-driven:** write a failing test first, then the code that makes it pass.
- **Self-documenting code:** clear names and small functions instead of comments; doc comments on public items.
- **Checks:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets`, `cargo test --workspace` and
  `cargo doc --workspace --no-deps` must pass with warnings treated as errors (same as CI).
- **Commits:** Conventional Commits on short-lived branches off `main`.

Full coding rules live in [`.claude/rules/`](.claude/rules/).

## License

Planned: MIT OR Apache-2.0 (license files not yet added).
