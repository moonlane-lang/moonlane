# ADR-0007 — Array Value Semantics via Deep-Clone at Bind Sites

**Date:** 2026-05-23  
**Status:** Accepted

---

## Context

`Value::Array` uses `Rc<RefCell<Vec<Value>>>` internally so that `array_push` can mutate the array in place through a shared pointer. This naturally gives arrays *reference semantics*: assigning an array variable to another name produces an alias, and mutations through either name are visible to both.

The Moonlane specification requires *value semantics*: assigning an array to a new name produces an independent copy, not an alias. The evaluator must reconcile these two requirements.

Two plausible approaches:

1. **Deep-clone at bind** — Call `deep_clone_value()` inside `env.define()` and `env.set()` whenever the value is an `Array`. Every binding gets its own `Rc`.
2. **Copy-on-write (CoW)** — Keep sharing until a mutation is attempted; clone on first write.

---

## Decision

**Deep-clone at every bind site.** `env.define()` and `env.set()` call `deep_clone_value()` unconditionally on `Value::Array`.

---

## Rationale

CoW is the correct long-term design (avoids redundant copies for read-only aliases) but adds complexity: every mutation path must check the reference count and clone if shared. For the PoC evaluator this overhead is unjustified.

Deep-clone is simple, correct, and produces the right semantics for all currently tested programs. The evaluator is intended to be replaced before production use; the simpler approach is appropriate here.

---

## Consequences

- `array_push` applied to a binding mutates through the `Rc<RefCell>` as expected; the clone was made at bind time so no aliasing exists.
- Function arguments receive their own copy; mutations inside the function do not affect the caller.
- Passing large arrays repeatedly is O(n) per call, not O(1). Acceptable for the PoC; CoW is the Epic-rewrite path.
- **Invariant to preserve:** `deep_clone_value()` must be called at every bind site that could store an `Array`. Adding a new binding path (e.g., a destructuring form) without calling `deep_clone_value()` silently reintroduces reference semantics.
