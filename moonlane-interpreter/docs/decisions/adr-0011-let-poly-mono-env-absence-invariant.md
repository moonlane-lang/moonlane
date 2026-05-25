# ADR-0011: Let-Polymorphism via mono_env Absence

**Status:** Accepted  
**Date:** 2026-05-25  
**Issue:** #10

---

## Context

Let-polymorphism allows an unannotated `let`-bound closure to be used at multiple
types in the same scope:

```moonlane
let id = fun(x) { x };
assert(id(42) == 42);   // Int
assert(id(true));       // Bool
```

`construct_call` already has a fast path for polymorphic callees: when a name is
absent from `ConstructCtx.env` but present in `scheme_env`, it calls
`instantiate_scheme_for_call` with fresh type variables. This path was built for
top-level `fun` declarations and works identically for let-poly closures.

## Decision

For a let-bound closure that generalises to a non-trivial type scheme:
1. **Do NOT** bind its name in `InferContext.mono_env` (Pass 1) or `ConstructCtx.env` (Pass 2).
2. **Only** bind it in `InferContext.poly_env` and `scheme_env`.

This makes the name absent from `env` while present in `scheme_env`, activating the
existing polymorphic dispatch path with zero new infrastructure.

`TypedExpr::GenericClosure` stores the untyped `Block` so the evaluator can use the
same runtime re-construction mechanism as `FunBody::Generic` (see ADR-0010).

## Invariant

**A name bound only in `poly_env`/`scheme_env` (absent from `mono_env`/`env`) is
always polymorphic.** Breaking this invariant — e.g., also binding in `mono_env` as
a monomorphic fallback — would cause `lookup()` to return a concrete `InferType` and
bypass the polymorphic instantiation path, silently making the binding monomorphic.

## Consequences

- **Scoped `poly_env`**: changed from `HashMap` to `Vec<HashMap>` so that local
  let-poly bindings don't leak across function scope boundaries. `push_scope` /
  `pop_scope` now maintain both `mono_env` and `poly_env` in lockstep.
- **Zero extra call-site code**: the construct_call fast path handles both top-level
  generic functions and let-poly closures identically.
- **Limitation**: only unannotated closures whose inferred type has free variables are
  generalised. Annotated closures (`let f: (Int) -> Int = fun(x) { x }`) remain
  monomorphic — the annotation pins the type.
