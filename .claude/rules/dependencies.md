---
paths:
  - "**/Cargo.toml"
  - "Cargo.lock"
  - "rust-toolchain.toml"
---
# Dependencies

- All deps in root `[workspace.dependencies]`; members `foo.workspace = true` (siblings too, never `path = "../x"`).
- Few deps, each justified in PR (why std insufficient, maintenance, license MIT/Apache compatible). Prefer std.
- `default-features = false`; enable only needed features.
- `windows` crate (latest 0.62): minimal features, pin exact `=0.62.x`. API changes per version, expect call-site fixes.
- `windows-reactor`: git dependency on windows-rs `master` at a fixed `rev`, because the release 0.100.0 lacks `AppProxy` and a public `App::run_with`. Move to a crates.io version as soon as a release has them. Update the `rev` on purpose only, and check the settings window after each update.
- Toolchain bump: update `rust-toolchain.toml` and `rust-version` together.
