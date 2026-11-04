# TypeAlike

TypeAlike gives system-wide inline autocomplete: ghost text at the caret in any app, and Tab accepts it. Rust, Windows first, also macOS. Inspired by Cotypist (https://cotypist.app/) and Cotabby (https://github.com/FuJacob/cotabby).

## Commands

- `cargo run`: the app (default member `crates/typealike`, bin `TypeAlike`).
- `cargo run --features playground`: the settings window then shows the Playground pane last, where a developer tries the suggestion model. A release build has no Playground pane (`PlaygroundPage::of_build`).
- One test: `cargo test -p typealike_core NAME`.
- Before each commit, run the CI checks. All must pass:
  - `cargo fmt --all --check`
  - `cargo clippy --workspace --all-targets` (`RUSTFLAGS=-D warnings`)
  - `cargo test --workspace`
  - `cargo doc --workspace --no-deps` (`RUSTDOCFLAGS=-D warnings`)
- To measure a change to the suggestions, or the first-key time after a model load, use the `prompt-eval` skill.
- After a change of the dependencies, run `python3 packaging/build_licenses.py`. It writes the third-party software of the About pane, and CI fails when the files are old.

## Build gotchas

- GPU backends (Windows, opt-in): `cargo run --features vulkan,cuda`. The default build runs models on the CPU only.
  - `vulkan` needs the Vulkan SDK (`VULKAN_SDK`).
  - `cuda` needs the CUDA Toolkit, and the CUDA DLLs on `PATH` at run time.
- Local build settings go in `.cargo/config.local.toml` (gitignored, loaded by `.cargo/config.toml`). Copy `.cargo/config.local.example.toml` to start. It sets:
  - the Ninja generator (the Visual Studio generator finds no CUDA toolset)
  - `CUDACXX`
  - a short target folder (the Vulkan shader build goes past `MAX_PATH`)
  - `CMAKE_CUDA_ARCHITECTURES` for the local GPU only (much faster CUDA build)
- CMake configures llama.cpp only once. After a change in `[env]` or a failed CMake step, run `cargo clean -p llama-cpp-sys-2`.
- The llama.cpp build needs CMake and libclang. It does not use OpenSSL (`.cargo/config.toml`), because llama-common.dll then needs OpenSSL DLLs that Windows does not have.
- Windows settings window: WinUI 3 through `windows-reactor`, a git dependency on windows-rs `master` at a fixed commit (the release 0.100.0 has no `AppProxy` and no public `App::run_with`).
  - It is framework-dependent: running TypeAlike needs the Windows App Runtime 2 installed.
  - The `self-contained` feature (`windows-reactor-setup` in `build.rs`) copies the runtime next to the program instead. It adds about 30 MB, and the Windows installers use it.
- macOS: TypeAlike needs Accessibility access. With `cargo run` from a terminal, grant it to the terminal app. The OCR also needs Screen & System Audio Recording, which `cargo run` also takes from the terminal app.
- Each program that loads a model calls `disable_metal_residency_sets` at the start of its `main` (the bin, `prompt_eval`, `first_key`). The reason is in `docs/llm.md`.

## Release

- Push a tag `vX.Y.Z` equal to the workspace version.
- `.github/workflows/release.yml` builds `TypeAlike-X.Y.Z-macos-arm64.dmg` (ad-hoc signature, no notarization) and two Windows installers, then makes a draft release.
  - `-windows-x64-vulkan-setup.exe` has Vulkan.
  - `-windows-x64-cuda-setup.exe` has Vulkan and CUDA with the CUDA runtime and cuBLAS DLLs.
- A manual run (`workflow_dispatch`) builds only the workflow artifacts.
- The scripts are in `packaging/`: `macos/build-dmg.sh` (app bundle from `Info.plist`, disk image), `windows/stage.ps1` (the files from the release folder, the Visual C++ runtime, the CUDA runtime, cuBLAS, then a check that Windows or the folder has each DLL that the built files load) and `windows/TypeAlike.iss` (Inno Setup, current user only, `%LOCALAPPDATA%\Programs\TypeAlike`).
- `release.yml` pins MSVC 14.44, CUDA, the Vulkan SDK and the CUDA GPU targets (`CMAKE_CUDA_ARCHITECTURES`).

## Debugging

- Log file: `TypeAlike.log` in the temp folder (`%TEMP%` on Windows, `$TMPDIR` on macOS), next to the terminal output.
  - Each run appends to it, so a crash log stays. When TypeAlike starts, it moves a log above 10 MB to `TypeAlike.previous.log`.
  - `TYPEALIKE_LOG` sets the levels in the syntax of `tracing_subscriber::filter::Targets` (default `info`), for example `debug` or `info,typealike_llm=debug`.
  - The llama.cpp logs have the target `llama-cpp-2`.
  - A panic also goes to the log.
  - The first log line of each run shows the version and the GPU features of the build.
- Settings file: `%LOCALAPPDATA%\TypeAlike\settings.json` on Windows, `~/Library/Application Support/TypeAlike/settings.json` on macOS. The default settings are in `docs/core/settings-window.md`.
- Earlier writing: `history.bin` next to `settings.json`. To reset the history, delete it and its key (`docs/core/earlier-writing.md`).

## Architecture

- `typealike_core` makes every decision and owns all UI state and text. It is pure Rust, so its tests run headless.
- The platform crates only translate OS events into `Controller` calls, and run the OS actions that core requests through the `Shell` trait.
- The crate rules are in `.claude/rules/architecture.md`.

## Behavior docs

`docs/` describes how each area behaves. Read the doc of an area before you change its behavior.

| Doc | Area |
|---|---|
| `docs/core/prompt.md` | Sections of the prompt, place, screen text, screen reads |
| `docs/core/earlier-writing.md` | Store, pools, examples, words of the answered messages, word bias, phrases, next-word bias, `history.bin` |
| `docs/core/typing.md` | Key rules, sessions, Enter, read of the field, held keys, shown suggestions, Escape, Tab, Tab hold |
| `docs/core/suggestions.md` | Length, typo rule and fix, the stops and the filters of a suggestion |
| `docs/core/ghost-text.md` | Caret looks, timing, scroll, `GhostView`, colors, mirror, placement |
| `docs/core/settings-window.md` | Rows, settings file, panes, energy stops, apps and websites, badge menu, model recommendation, permissions, About pane, check for updates, third-party licenses, setup, UI languages |
| `docs/windows.md` | `typealike_windows`: overlay, hooks, caret reader, UIA, screen text, watches, settings and setup windows, autostart |
| `docs/macos.md` | `typealike_macos`: permissions, AX caret, overlay, mirror, injection, spelling, setup window, autostart |
| `docs/llm.md` | `typealike_llm`: backends, warm-up, token healing, typo fixes, stops, phrase ends, cache |
| `docs/icons.md` | Logo, app icons, tray icon and menu bar extra, `assets/build_icons.py` |

When a commit changes a behavior, update the doc of its area in the same commit. Do not add behavior to `CLAUDE.md`. It keeps only commands, gotchas and pointers. Write each doc as short bullets under headings, in ASD-STE100.

## Rules

`.claude/rules/` holds the coding rules. `git.md` and `ste100.md` always load. The other rules load with the files that they match.

- `ste100.md`: Simplified Technical English for all prose and UI strings.
- `git.md`: branches and Conventional Commits.
- `architecture.md`: crates, dependency direction, logic in core.
- `tdd.md`: test-first workflow and where tests go.
- `clean-code.md`, `rust.md`: naming, structure, API, errors, docs, modules.
- `dependencies.md`: Cargo and the toolchain.
- `windows.md`: FFI, threads, runtime invariants.
- `windows-ui.md`, `macos-ui.md`: the look of each OS.
- `ui-text.md`: what each UI text must say, the translations and the test of the writing rules.

## UI design

- `PRODUCT.md` (users, voice, terms, principles) and `DESIGN.md` (the look comes from the OS) give the context of the `impeccable` skill. Its briefs for the settings window, the overlay and the menus are in `.impeccable/surfaces/`.
- Use `/impeccable clarify` for UI text, and `audit` and `critique` for a review against `windows-ui.md` and `macos-ui.md`.
- The detector, the design hook, `live` and `generate` of `impeccable` read only web files, so they do not apply here.
