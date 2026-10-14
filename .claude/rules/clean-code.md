---
paths:
  - "**/*.rs"
---
# Clean code

## Self-documenting, no comments
- Code says what/how; names carry intent. Comments rot, compiler doesn't check them. Need one? Refactor first:
  - magic value → named `const` (`MAX_SUGGESTION_CHARS`)
  - complex condition → named bool or predicate fn (`let caret_moved = ...`)
  - commented block → extracted fn named after the comment
  - implicit unit/meaning → type (`Duration`, newtype `PhysicalPx`) or enum
- Allowed only: `///`/`//!` API docs (what + contract, not how); `// SAFETY:`; non-obvious *why* that code can't express (OS quirk, workaround, perf tradeoff), one line; `let _ =` justification; `#[expect(reason)]`.
- Never: restating code, commented-out code, changelogs, author/TODO without issue ref, section banners.

## Naming
- Full words; no abbreviations or single letters (except `i` in tiny closures/math). Searchable, pronounceable.
- No types in names (`name_str`, `vec_items`). No `Utils`/`Helper`/`Manager`/`Base`/`Data`/`Info` grab-bags: name by responsibility.
- Fns verbs (`accept_word`), types nouns, bools `is_`/`has_`/`should_`. Units in type, else in name (`delay_ms`).
- Length scales with scope. Same concept = same word everywhere; rename on meaning change.

## Structure
- Max 3 nesting levels. Flatten: early return, `?`, `let ... else`, guard clauses, extract fn, invert condition.
- Fn does one thing at one abstraction level; fits on screen (~40 lines). >4 params → struct/builder.
- No `bool` params; use enum (`Accept::Word`, not `accept(true)`).
- Make illegal states unrepresentable: enums with data over flags + optionals.
- Pure core, effects at edges: decide in pure fns, act in thin shell.
- Composition over inheritance-like trait hierarchies; small focused traits.
- Dependency injection: pass collaborators via constructor/param; no hidden globals/singletons.

## Abstraction & change
- YAGNI: no speculative features, config, generics, or extension points.
- Abstract only for real seam: OS/IO boundary, test seam, or ≥2 concrete impls. Duplication beats wrong abstraction; extract on third repeat.
- Abstractions add coupling; each must remove more complexity than it adds.
- Delete dead code. Leave touched code cleaner. Optimize only after measuring.
