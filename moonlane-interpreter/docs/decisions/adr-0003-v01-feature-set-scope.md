---
id: decision-3
title: "v0.1 Feature Set Scope"
date: '2026-04-04'
status: accepted
---
## Context

Before writing the interpreter we needed to draw a firm boundary: what exactly is v0.1? The language spec had many open and partially-designed areas. Implementing an under-designed feature risks having to break it later as the design matures.

The guiding question for every candidate feature was: *"If we implement this now, will we regret the design decision later?"* If the answer was uncertain, the feature was deferred.

## Decision

**Chosen: implement only fully-specified features, defer the rest.**

We scoped v0.1 to the features already fully designed in the spec, plus a small set of roadmap items whose design was settled enough to commit to without risk of future breakage. Everything else was explicitly deferred.

**Included:** the full spec as it stood at this date — primitives, variables, functions, closures, structs, enums, traits, pattern matching, arrays, tuples, `Perhaps<T>`, `Result<T,E>`, `?`, `as`/`From`, `loop`/`break`/`continue`, compound assignment, associated functions, `mut self`, closure type signatures, panics, and the built-in functions. See [docs/public/spec.md](../../../docs/public/spec.md) for the authoritative description of each.

**Deferred:** module system, visibility, `UInt`, string interpolation, trait objects, derived traits, operator overloading traits, `?` error coercion, `List<T>`, and integer overflow semantics. These are tracked in `Backlog.md`.

The deferred features share a common property: their *design* is not yet settled — syntax, semantics, or interaction with other features is still open. Implementing them now would mean either implementing a known-incomplete design or having to revisit and break the implementation later. All of the included features were fully specified before implementation began.

A secondary criterion was additive safety: deferred features can all be added later without requiring changes to existing v0.1 programs. No v0.1 syntax will be invalidated by adding a module system, `dyn Trait`, or derive macros.

## Consequences

- All v0.1 programs are single-file (no module system).
- `Array<T>` / `T[]` is the only sequence type; `List<T>` does not exist yet.
- String formatting requires explicit conversion functions (`int_to_string`, etc.) and `+` concatenation.
- The interpreter can be written entirely against `Language Spec.md` with no ambiguity about scope.

## References

- Spec: [docs/public/spec.md](../../../docs/public/spec.md)
- Deferred features: [docs/internal/rfcs/](../../../docs/internal/rfcs/) (RFC stubs for all deferred items)
