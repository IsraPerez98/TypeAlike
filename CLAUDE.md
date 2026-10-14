# GhostType

OSS Windows-first Cotypist clone (https://cotypist.app/). System-wide inline autocomplete: ghost text at caret in any app, Tab accepts. Rust.

Proven plumbing in PoC `C:\Users\Israel\ghosttype-poc` (UIA caret, layered overlay, LL hooks, SendInput). Port deliberately, no wholesale copy.

## Build
- Cargo not on PATH: `$env:USERPROFILE\.cargo\bin\cargo.exe`.
- Toolchain pinned `rust-toolchain.toml`; bump with `rust-version`.
- `cargo run` = app (default member `crates/ghosttype`, bin `GhostType`).
- Pre-commit = CI: `cargo fmt --all --check`; `cargo clippy --workspace --all-targets` (`RUSTFLAGS=-D warnings`); `cargo test --workspace`; `cargo doc --workspace --no-deps`.

## Layout
Virtual workspace, flat `crates/`. Never nest crates in `src/`.
- `ghosttype_core`: engine, matching, accept state machine, config model. Pure Rust: no OS API, no `windows`, no `unsafe`. Full unit tests.
- `ghosttype_windows`: hooks, UIA, overlay, `SendInput`, tray. ALL `unsafe`/FFI here, behind safe wrappers.
- `ghosttype`: bin. Wiring, config, logging, event loop. Thin, no logic.

Deps point inward: app → windows → core. Core defines traits (text source, suggestion provider, injector); platform implements. New features (AI provider, settings UI, other OS) = new crate/module behind traits. New crate when clear boundary/own deps/compile time; name `ghosttype_<area>`.

## Deps
- All in root `[workspace.dependencies]`, members use `foo.workspace = true` (siblings too, never `path = "../x"`).
- Few deps, each justified. Prefer std.
- `windows` crate (latest 0.62): minimal features, pin exact `=0.62.x`; API changes per version, expect call-site fixes.
- Errors: `thiserror` in libs, `anyhow` + `.context()` in bin. Logging: `tracing` structured fields, never `println!`.

## Code standards
(Rust API Guidelines, MS Pragmatic Rust Guidelines, Clippy, Cargo Book)
- Lints in `[workspace.lints]`, crates `[lints] workspace = true`. Clippy pedantic+cargo+restriction picks; CI warnings = errors.
- Silence lint: `#[expect(lint, reason = "...")]`, never bare `#[allow]`.
- Naming: `UpperCamelCase` types/traits, `snake_case` fn/var/mod, `SCREAMING_SNAKE_CASE` const. Acronyms as words (`Uia`, `Hwnd`). `as_` cheap borrow, `to_` expensive, `into_` consume. Getters no `get_`. Ctors `new`/`with_*`; fallible → `Result`.
- Errors: specific type per op/area, no global enum. `impl From` + `?`, not repeated `map_err`. Msgs lowercase, no period. Never silently drop `Result` (`let _ =` needs comment).
- Panic only for bugs. No `unwrap()` outside tests; `expect("why can't fail")`. Release `panic = "abort"`, no recovery.
- `unsafe`: only `ghosttype_windows`, minimal blocks, `// SAFETY:` comment each, safe wrapper (RAII + `Drop` for handles). `unsafe fn` only if misuse = UB. Never fake `Send`/`Sync`.
- Docs: `//!` crate, `///` every pub item (`missing_docs`). `# Errors`/`# Panics`/`# Safety` when apply. First doc para one line.
- All pub types `Debug`. Redact secrets + user-typed text in `Debug`/logs.
- Visibility: private → `pub(crate)` → `pub`. One pub path per item; no glob re-exports, no preludes.
- No globals/statics; pass state. Win32 callback thread-local state: one place only.
- OS/IO behind traits → core tests run without desktop.
- UTF-8: `char`/grapheme ops, never byte-slice user text.
- Small fns, `match`/iterators, clone only when needed. No `RefCell` borrow across callback/nested message loop.

## Runtime invariants (from PoC)
- Hooks, timers, window msgs on main thread. Hook callbacks fast (Windows drops slow LL hooks): no UIA, no `SendInput` inside; post msg.
- UIA on worker thread (COM MTA). Requests carry ID; stale results dropped.
- Tag own `SendInput` via `dwExtraInfo`, ignore only those (not all injected).
- Skip password fields + own process. Per-monitor-v2 DPI aware → physical px.
- `HWND` not `Send`: cross threads as `isize`.

## Git
- `main` default. Short feature branches, Conventional Commits (`feat:` `fix:` `refactor:` `docs:` `test:` `chore:`), small PRs.
- Commit `Cargo.lock` (app). Never `target/`, exes.
- `CLAUDE.original.md` = uncompressed source, local only, gitignored.
