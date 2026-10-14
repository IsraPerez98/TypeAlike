---
paths:
  - "**/*.rs"
---
# Rust

## Lints
- Lints in root `[workspace.lints]`; every crate `[lints] workspace = true`. Clippy pedantic+cargo+selected restriction; warnings = errors.
- Silence: `#[expect(lint, reason = "...")]`, narrowest scope. Never bare `#[allow]`.

## API & naming
- `UpperCamelCase` types/traits, `snake_case` fn/var/mod, `SCREAMING_SNAKE_CASE` const. Acronyms as words (`Uia`, `Hwnd`).
- Conversions: `as_` cheap borrow, `to_` expensive, `into_` consume. Getters no `get_`. Ctors `new`/`with_*`; fallible → `Result`.
- Impl std traits where meaningful: `Debug` (all pub types), `Clone`, `PartialEq`, `Default`, `From`/`TryFrom`, `Display` for user-facing.
- Params borrow (`&str`, `&[T]`, `impl AsRef`), return owned. `#[must_use]` on pure fns/builders where ignoring is a bug.
- Newtypes for domain values/units; never raw `usize`/`i32` with implicit meaning.
- No `as` numeric casts: `From`/`TryFrom`. Checked/saturating arithmetic where overflow possible.
- Generics only when ≥2 types used; `dyn Trait` for runtime plugins.

## Errors & panics
- `thiserror` in libs, `anyhow` + `.context()` in bin.
- Specific error type per op/area, no global enum. `impl From` + `?`, not repeated `map_err`.
- Msgs lowercase, no period, no secrets/user text. Never silently drop `Result` (`let _ =` needs why comment).
- Panic only for bugs. No `unwrap()` outside tests; `expect("why can't fail")`. Release `panic = "abort"`.

## Docs
- `//!` per crate, `///` every pub item. First para one line. `# Errors`/`# Panics`/`# Safety` when apply. Examples as doctests.
- Docs describe contract, not implementation.

## Visibility & modules
- Private → `pub(crate)` → `pub`. One pub path per item. No glob re-exports, no preludes, no `use super::*` outside tests.
- Module per concept; file per module when it grows.

## Data & safety
- Redact secrets and user-typed text in `Debug`/logs.
- User text: `char`/grapheme ops, never byte-slice. Positions as char/grapheme indices, explicit type.
- Logging: `tracing` structured fields, never `println!`/`dbg!`.
- Clone only when needed. Iterators/`match` over index loops and `if` chains.
- Concurrency: message passing (channels) over shared `Mutex`. Never hold lock or `RefCell` borrow across callback, `await`, or nested message loop. Never fake `Send`/`Sync`.
