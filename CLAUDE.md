# GhostType

OSS Windows-first Cotypist clone (https://cotypist.app/). System-wide inline autocomplete: ghost text at caret in any app, Tab accepts. Rust.

## Build
- Toolchain pinned `rust-toolchain.toml`; bump with `rust-version`.
- `cargo run` = app (default member `crates/ghosttype`, bin `GhostType`).
- Pre-commit = CI, all must pass: `cargo fmt --all --check`; `cargo clippy --workspace --all-targets` (`RUSTFLAGS=-D warnings`); `cargo test --workspace`; `cargo doc --workspace --no-deps`.
- GPU backends (Windows, opt-in): `cargo run --features vulkan,cuda`. The default build runs models on the CPU only. `vulkan` needs the Vulkan SDK (`VULKAN_SDK`). `cuda` needs the CUDA Toolkit and the CUDA DLLs on `PATH` at run time. Local build settings go in `.cargo/config.local.toml` (gitignored, loaded by `.cargo/config.toml`). Copy `.cargo/config.local.example.toml` to start. It sets the Ninja generator (the Visual Studio generator finds no CUDA toolset), `CUDACXX`, a short target folder (the Vulkan shader build goes past `MAX_PATH`), and `CMAKE_CUDA_ARCHITECTURES` for the local GPU only (much faster CUDA build). CMake configures llama.cpp only one time: after a change in `[env]` or a failed CMake step, run `cargo clean -p llama-cpp-sys-2`.
- Settings file: `%LOCALAPPDATA%\GhostType\settings.json` on Windows, `~/Library/Application Support/GhostType/settings.json` on macOS (the General pane writes it).
- macOS autostart: the General pane writes the launchd agent `~/Library/LaunchAgents/io.github.israperez98.ghosttype.plist`. It starts the program that wrote it.

## Layout
Virtual workspace, flat `crates/`.
- `ghosttype_core`: engine, matching, accept state machine, prompt (system prompt, app and window, text around the caret, clipboard), settings model, Models, Playground and General panes. Pure Rust, no OS. Platforms read the desktop through the `Desktop` trait.
- `ghosttype_windows`: hooks, UIA, overlay, `SendInput`, tray, settings window. All FFI.
- `ghosttype_macos`: event tap, Accessibility (AX), overlay panel, CGEvent injection, menu bar extra, settings window. All FFI. Needs Accessibility access (`cargo run` from a terminal: grant it to the terminal app).
- `ghosttype_models`: Hugging Face search, downloads and the local models directory (`ModelHub`). HTTPS + file IO, no FFI.
- `ghosttype_llm`: runs GGUF models with llama.cpp (`llama-cpp-2`) on one worker thread (`ModelRunner`), on the backend that each load names (CPU, Vulkan, CUDA). Metal on Apple silicon. Refuses a GPU load when the whole model does not fit in free GPU memory. Heals the last word of each prompt (token healing). Keeps the last prompt in the llama.cpp cache, so the next prompt decodes only the tokens that it adds. For a recurrent model (Qwen3.5), it also saves the state before each of the last words, so a backspace across a word does not decode the full prompt again. Sends the text of a completion while it grows. With the "Show words early" setting, the engine shows its complete words at once. Build needs CMake and libclang.
- `ghosttype`: bin. Wiring, config, logging, event loop.

## Rules
`.claude/rules/` (binding; path-scoped ones load with matching files):
- `tdd.md` test-first workflow · `clean-code.md` naming, structure, no comments · `architecture.md` crate boundaries, traits
- `portability.md` logic in core; platform crates only translate, so OS ports never touch core
- `rust.md` API, errors, docs, types · `dependencies.md` Cargo · `windows.md` FFI, threads, runtime invariants · `windows-ui.md` Windows 11 look & feel (Fluent) · `macos-ui.md` macOS look & feel (Apple HIG, Liquid Glass) · `git.md` · `ste100.md` Simplified Technical English for prose and UI strings
