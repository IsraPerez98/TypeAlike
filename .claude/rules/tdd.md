# Test-Driven Development

- Red → green → refactor. No production code without a failing test demanding it. Run it, see it fail for the right reason.
- Smallest code to pass, then refactor with tests green. Never refactor on red.
- Bug fix starts with a test reproducing the bug.
- Done = new behavior tested + full pre-commit suite green. Untested code is not mergeable.
- Test behavior via public API, not internals. Refactors must not break tests.
- One behavior per test. Name states behavior: `tab_accepts_full_suggestion`, `stale_result_is_dropped`. No `test1`.
- Arrange / act / assert, blank-line separated. No logic (loops, branches) in tests; use table cases for variants.
- Deterministic: no sleeps, real clock, network, randomness, or desktop. Inject time/IO via traits.
- Fakes (small hand-written trait impls) over mock frameworks. Builders/helpers for repetitive setup.
- Unit tests in `#[cfg(test)] mod tests` beside code; cross-crate behavior in `crates/*/tests/`. Pub examples as doctests.
- `typealike_core`: full coverage, every branch of state machine and matching.
- `typealike_windows`: keep logic out of FFI so it's testable as pure fns; thin FFI wrappers checked manually, document manual steps in PR.
- Tests are code: same naming/clean rules. `unwrap`/`expect` allowed.
