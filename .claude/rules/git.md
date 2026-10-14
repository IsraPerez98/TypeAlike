# Git

- `main` default, always green. Short feature branches, small focused PRs.
- Conventional Commits: `feat:` `fix:` `refactor:` `docs:` `test:` `chore:`. Imperative, lowercase subject.
- Commit test + implementation together (or test first); never a red commit on `main`.
- Separate refactors from behavior changes.
- Commit `Cargo.lock`. Never `target/`, exes, PDBs, secrets.
