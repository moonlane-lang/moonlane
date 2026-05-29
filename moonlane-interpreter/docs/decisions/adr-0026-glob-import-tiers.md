---
id: ADR-0026
title: Glob import tier model for priority-based std::core resolution
status: accepted
date: 2026-05-29
issues: ["#206"]
---

## Context

`std::core` is auto-imported as a glob into every module. Without special handling, any user `import path::*` glob that exports a name also in `std::core` would trigger T0011 (conflicting globs). That would force users to write explicit imports to suppress the error, defeating the purpose of auto-import.

The original suppression approach was to skip T0011 when one of the conflicting globs was `std::` prefixed — a string-prefix check. This is brittle (fragile to renaming) and conflates priority with origin string matching.

## Decision

Introduce a `GlobTier` enum with two variants:

- `Std` — inserted automatically by the runtime (e.g. `std::core` auto-import). Lowest priority.
- `User` — explicit `import path::*` in user source. Higher priority.

`ModuleScope.globs` changes from `Vec<Vec<String>>` to `Vec<(GlobTier, Vec<String>)>`.

Conflict rules:
- A `User` glob silently wins over a `Std` glob for the same name — no error, no warning.
- Two `User` globs exporting the same name are a conflict error (T0011) only if that name is actually referenced.

T0011 now fires only for same-tier conflicts. Cross-tier conflicts are resolved by priority, not rejected.

## Alternatives Considered

**String-prefix check (`std::` → suppress T0011)** — works for current cases but breaks if the stdlib is reorganised or a user creates a module named `std`. Conflates name-based origin with priority semantics.

**Separate auto-import list, skip during conflict detection** — requires maintaining a separate set of auto-imported paths alongside `globs`. More state, same result.

## Consequences

- The tier model is extensible: a third tier (e.g. `Prelude`) could be added without changing existing conflict logic
- Code that processes globs must destructure `(GlobTier, path)` tuples — all such sites in `path_normalizer.rs` and `typechecker/mod.rs` have been updated
- User-visible behaviour: a user `import util::*` that exports `Perhaps` silently takes precedence over `std::core::Perhaps` with no error — this is intentional and matches the spec's "User glob wins over Std glob" rule
