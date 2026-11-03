# Contributing to TypeAlike

Thank you for your help.

To report a bug or ask a question, open an [issue](https://github.com/IsraPerez98/TypeAlike/issues).

## Build from source

### Prerequisites

- [rustup](https://rustup.rs/). `rust-toolchain.toml` sets the toolchain version, and rustup installs it at the first
  build.
- [CMake](https://cmake.org/) and libclang. They build llama.cpp, which runs the models.
- Windows: the MSVC build tools and [LLVM](https://llvm.org/) (for libclang). To run a normal build, also install the
  Windows App Runtime 2.
- macOS: the Xcode Command Line Tools.

### Commands

```sh
cargo run                          # the app (bin TypeAlike of crates/typealike)
cargo run --features playground    # the settings window also shows the Playground pane, to try the model
cargo test -p typealike_core NAME  # one test
```

On macOS, `cargo run` from a terminal takes the permissions of the terminal app. Give Accessibility to the
terminal app. For screen text, also give it Screen & System Audio Recording.

### GPU backends on Windows

The default build runs the models on the CPU only. The GPU backends are opt-in features:

```sh
cargo run --features vulkan,cuda
```

- `vulkan` needs the [Vulkan SDK](https://vulkan.lunarg.com/) (`VULKAN_SDK`).
- `cuda` needs the CUDA Toolkit, and the CUDA DLLs on `PATH` at run time.
- `self-contained` copies the Windows App Runtime next to the program. It adds about 30 MB, and the installers
  use it.

On macOS, each build runs the models with Metal.

### Local build settings

Put your machine's build settings in `.cargo/config.local.toml`. Git ignores this file, and `.cargo/config.toml`
loads it. Copy [`.cargo/config.local.example.toml`](.cargo/config.local.example.toml) to start. It sets the Ninja
generator, `CUDACXX`, a short target folder and `CMAKE_CUDA_ARCHITECTURES`. The comments in the example explain each
value.

CMake configures llama.cpp only once. After a change in `[env]`, or after a failed CMake step, run:

```sh
cargo clean -p llama-cpp-sys-2
```

More build notes are in [CLAUDE.md](CLAUDE.md#build-gotchas).

## Crates

| Crate | Purpose |
|---|---|
| `crates/typealike` | The `TypeAlike` binary: wiring, data folders, settings load, logging, event loop |
| `crates/typealike_core` | All decisions and all UI state: suggestions, Tab, key rules, prompt, earlier writing, settings model, panes, setup, UI texts |
| `crates/typealike_windows` | Win32 and UI Automation: hooks, caret reader, overlay, badge, input, tray, settings window, screen text, autostart |
| `crates/typealike_macos` | AppKit and Accessibility: event tap, caret reader, overlay, badge, input, menu bar, settings window, screen text, launch agent |
| `crates/typealike_models` | The Hugging Face search, the model downloads and the local models folder |
| `crates/typealike_llm` | Runs GGUF models with llama.cpp |
| `crates/typealike_history` | The user's earlier writing, in one encrypted file |
| `crates/typealike_settings` | The settings file and its format |

`typealike_core` is pure Rust, so its tests run headless. The platform crates only translate OS events into
`Controller` calls, and run the OS actions that core requests through the `Shell` trait. The rules for the crates
and the direction of the dependencies are in [`.claude/rules/architecture.md`](.claude/rules/architecture.md).

## Workflow

### Tests first

1. Write a test that fails for the right reason.
2. Write the smallest code that makes it pass.
3. Refactor while the tests stay green.

A bug fix starts with a test that shows the bug. A behavior that core cannot test headless is in the wrong crate.
The full rules are in [`.claude/rules/tdd.md`](.claude/rules/tdd.md).

### Checks

Before each commit, run the CI checks. All must pass:

```sh
cargo fmt --all --check
RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

[`ci.yml`](.github/workflows/ci.yml) runs them on Windows. On Linux, it also runs clippy and the tests of the crates
that have no OS code. The macOS job runs only when you start the workflow by hand.

### Commits and branches

- `main` is always green. Use short branches and small pull requests.
- Use [Conventional Commits](https://www.conventionalcommits.org/): `feat:`, `fix:`, `refactor:`, `docs:`, `test:`,
  `chore:`. Write the subject in the imperative and in lowercase.
- Put a test and its code in the same commit, or the test first. Never commit a failing test to `main`.
- Keep a refactor and a change of behavior in separate commits.
- Commit `Cargo.lock`. Never commit `target/`, programs, PDB files or secrets.

### Behavior docs

[`docs/`](docs/) describes how each area behaves. The table in [CLAUDE.md](CLAUDE.md#behavior-docs) lists the doc
for each area. Read the doc of an area before you change its behavior. When a commit changes a behavior, update
the doc of its area in the same commit.

### Changes to the suggestions

Measure each change to the prompt, the screen text, the earlier writing, the typo rule or the suggestion filters with
the `prompt_eval` example of `typealike_llm`.
[`.claude/skills/prompt-eval/SKILL.md`](.claude/skills/prompt-eval/SKILL.md) explains how to run it.

## Coding and writing rules

The rules are in [`.claude/rules/`](.claude/rules/):

- [`clean-code.md`](.claude/rules/clean-code.md) and [`rust.md`](.claude/rules/rust.md): names, structure, API,
  errors, docs and modules.
- [`windows.md`](.claude/rules/windows.md): FFI, threads and the rules of the Windows runtime.
- [`dependencies.md`](.claude/rules/dependencies.md): Cargo and the toolchain.
- [`ste100.md`](.claude/rules/ste100.md): Simplified Technical English (ASD-STE100) for all prose. This includes
  docs, commit messages, code comments and UI text.
- [`ui-text.md`](.claude/rules/ui-text.md): what each UI text must say, and the translations. The terms are in
  [PRODUCT.md](PRODUCT.md#terms).
- [`windows-ui.md`](.claude/rules/windows-ui.md) and [`macos-ui.md`](.claude/rules/macos-ui.md): the look on each OS.
  [DESIGN.md](DESIGN.md) lists the parts that TypeAlike draws itself.

## Debugging and releases

[CLAUDE.md](CLAUDE.md) lists the locations of the log file and the settings file, the log levels (`TYPEALIKE_LOG`),
and the release steps.
