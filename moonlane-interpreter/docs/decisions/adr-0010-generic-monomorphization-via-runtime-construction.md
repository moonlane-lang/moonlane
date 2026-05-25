# ADR-0010: Generic Monomorphization via Runtime Re-Construction

**Status:** Accepted  
**Date:** 2026-05-25  
**Issues:** #5, #7, #9

---

## Context

Generic functions (`fun id<T>(x: T) -> T`) must produce typed AST nodes for every
concrete call-site type. Two strategies exist:

**Option A — Ahead-of-time monomorphization**: during Pass 2 construction, collect all
call sites, determine the concrete type at each, and emit a specialised `TypedBlock`
per unique instantiation. Requires a second scan of the program or a deferred queue.

**Option B — Runtime re-construction**: store the untyped `Block` in
`FunBody::Generic(block)`. At every call site the evaluator runs a mini construction
pass on the untyped block using the concrete argument types, producing a fresh
`TypedBlock` that is evaluated immediately.

## Decision

Option B — runtime re-construction.

The construction pass is already available at runtime (it is the second half of
`typechecker::check`). Re-invoking it with a fresh `ConstructCtx` seeded with the
call-site types is simple and requires no new infrastructure.

The same mechanism extends naturally to let-polymorphic closures via
`ClosureBody::Untyped(Block)` / `TypedExpr::GenericClosure`.

## Consequences

- **Correct**: each call site gets a fully typed block independent of all others.
- **Simple**: no monomorphization cache, no call-site tracking pass.
- **Performance cost**: every call to a generic function re-runs the construction pass
  on the body — O(body size) per call, not amortized. Acceptable for the tree-walk
  interpreter; a future compiler backend must pre-monomorphize.
- **Future work**: if performance becomes a concern, a call-site cache keyed on the
  concrete type tuple can be added without changing the interface.
