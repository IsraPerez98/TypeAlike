# GhostType

OSS Windows-first Cotypist clone (https://cotypist.app/). System-wide inline autocomplete: ghost text at caret in any app, Tab accepts. Rust.

## Build
- Toolchain pinned `rust-toolchain.toml`; bump with `rust-version`.
- `cargo run` = app (default member `crates/ghosttype`, bin `GhostType`).
- Pre-commit = CI, all must pass: `cargo fmt --all --check`; `cargo clippy --workspace --all-targets` (`RUSTFLAGS=-D warnings`); `cargo test --workspace`; `cargo doc --workspace --no-deps`.
- GPU backends (Windows, opt-in): `cargo run --features vulkan,cuda`. The default build runs models on the CPU only. `vulkan` needs the Vulkan SDK (`VULKAN_SDK`). `cuda` needs the CUDA Toolkit and the CUDA DLLs on `PATH` at run time. With MSVC, build CUDA from a VS developer prompt with `CMAKE_GENERATOR=Ninja` and `CUDACXX` set, because the Visual Studio generator finds no CUDA toolset. Keep the target folder path short (`CARGO_TARGET_DIR`), because the Vulkan shader build goes past `MAX_PATH`.
- Settings file: `%LOCALAPPDATA%\GhostType\settings.json` (the General pane writes it).

## Layout
Virtual workspace, flat `crates/`.
- `ghosttype_core`: engine, matching, accept state machine, settings model, Models, Playground and General panes. Pure Rust, no OS.
- `ghosttype_windows`: hooks, UIA, overlay, `SendInput`, tray, settings window. All FFI.
- `ghosttype_macos`: event tap, Accessibility (AX), overlay panel, CGEvent injection, menu bar extra, settings window. All FFI. Needs Accessibility access (`cargo run` from a terminal: grant it to the terminal app).
- `ghosttype_models`: Hugging Face search, downloads and the local models directory (`ModelHub`). HTTPS + file IO, no FFI.
- `ghosttype_llm`: runs GGUF models with llama.cpp (`llama-cpp-2`) on one worker thread (`ModelRunner`), on the backend that each load names (CPU, Vulkan, CUDA). Metal on Apple silicon. Refuses a GPU load when the whole model does not fit in free GPU memory. Build needs CMake and libclang.
- `ghosttype`: bin. Wiring, config, logging, event loop.

## Rules
`.claude/rules/` (binding; path-scoped ones load with matching files):
- `tdd.md` test-first workflow · `clean-code.md` naming, structure, no comments · `architecture.md` crate boundaries, traits
- `portability.md` logic in core; platform crates only translate, so OS ports never touch core
- `rust.md` API, errors, docs, types · `dependencies.md` Cargo · `windows.md` FFI, threads, runtime invariants · `windows-ui.md` Windows 11 look & feel (Fluent) · `macos-ui.md` macOS look & feel (Apple HIG, Liquid Glass) · `git.md` · `ste100.md` Simplified Technical English for prose and UI strings
