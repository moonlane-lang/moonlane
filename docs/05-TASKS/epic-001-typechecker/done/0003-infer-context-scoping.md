# Task 0003: InferContext — Scoping and Environment Introspection

**Status:** done  
**Epic:** epic-001-typechecker  
**Component:** typechecker  
**Spec Link:** docs/01-SPEC/LANGUAGE-SPEC.md#32-type-inference  
**Blocked By:** 0002  
**Blocks:** 0005

## What

Fix two gaps in `InferContext` that will cause incorrect behaviour in Phase 8
as soon as the typechecker walks real, nested ASTs.

Both additions are small but architectural — getting them wrong means rewriting
the Phase 8 AST walk later.

---

## Gap 1 — Scoping

### Problem

`mono_env` is a flat `HashMap<String, InferType>`. When the typechecker enters
a function body and binds its parameters, those bindings persist for the rest of
inference. A parameter `x: Int` in `foo` leaks into the environment when `bar`
is checked next, and a local `let` inside a block stays in scope after the block
exits. Neither is correct.

### Solution

Replace the flat `mono_env` with a scope stack:

```rust
mono_env: Vec<HashMap<String, InferType>>,
```

Add two methods:

```rust
/// Enter a new lexical scope (e.g. a function body or block).
pub fn push_scope(&mut self)

/// Exit the current scope, removing all bindings introduced in it.
pub fn pop_scope(&mut self)
```

`bind_mono` inserts into the top-most scope. `lookup` searches from top to
bottom (innermost wins). `poly_env` does not need scoping because polymorphic
bindings are only created at the top level (let-bound names, function
declarations).

### Example

```
push_scope()          ← enter function body
  bind_mono("x", Int) ← parameter
  bind_mono("y", Bool)
  lookup("x") → Some(Int)
pop_scope()           ← exit function body
lookup("x") → None    ← gone
```

---

## Gap 2 — Environment Free Variables

### Problem

`generalize(ty, env_free_vars)` requires the caller to supply the set of type
variables that are still free in the *current environment*, so that those
variables are not captured in a `∀` quantifier. In Phase 7 tests this was always
passed as an empty `HashSet`, which is incorrect in general: it over-generalises,
capturing variables that are still being solved at an outer scope.

### Solution

Add a method to `InferContext` that computes the union of free variables across
all current mono bindings:

```rust
/// Collect all type variables that appear free in the current environment.
/// Used by generalize() to avoid over-capturing variables still being solved.
pub fn env_free_vars(&self) -> HashSet<TypeVar>
```

Implementation: iterate all scopes in `mono_env`, apply `free_vars(ty)` to
each binding, and union the results.

`poly_env` bindings need not be included because their quantified variables
are already captured — only the *free* variables of their `ty` fields matter,
and those are zero by definition (they are fully quantified).

### Example

```
bind_mono("x", Var(?t0))  ← ?t0 is free in the env
bind_mono("y", Int)        ← no free vars

env_free_vars() → { ?t0 }

generalize(Fun(?t0, ?t1) -> ?t1, { ?t0 })
  → ∀?t1. Fun(?t0, ?t1) -> ?t1   ← ?t0 not captured, ?t1 is
```

---

## Acceptance Criteria

- [ ] `mono_env` is a scope stack (`Vec<HashMap<String, InferType>>`)
- [ ] `push_scope()` adds a new empty scope
- [ ] `pop_scope()` removes the innermost scope
- [ ] `bind_mono` inserts into the top scope
- [ ] `lookup` searches scopes innermost-first; inner bindings shadow outer ones
- [ ] `poly_env` remains a flat `HashMap` (top-level only)
- [ ] `env_free_vars() -> HashSet<TypeVar>` collects free vars across all scopes
- [ ] All existing `phase_7_infer_context` tests still pass (no regressions)
- [ ] New tests cover: scope isolation, shadowing, env_free_vars with and without free vars

## Notes

- `push_scope` / `pop_scope` must be called in matched pairs — consider
  documenting this invariant clearly
- The initial state should have one root scope already pushed (so `bind_mono`
  works without an explicit `push_scope` call at startup)
- `solve()` can remain as-is — it consumes the context, so stale scopes
  are irrelevant after solving
