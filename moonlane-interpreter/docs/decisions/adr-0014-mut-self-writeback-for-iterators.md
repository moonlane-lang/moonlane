---
id: decision-14
title: "Mut-Self Writeback for Iterator Advancement in For-In"
date: '2026-05-25'
status: accepted
---

## Context

`Iterable<T>` requires `fun next(mut self) -> Perhaps<T>`. When `for-in` calls `iterator.next()`, the iterator's internal state (e.g., a `current_index` field) must advance. Because the evaluator uses value semantics — every binding stores an independent deep-cloned copy — calling `iterator.next()` produces a mutated copy of the iterator but does not automatically update the binding in the enclosing scope.

Without an explicit writeback, each `for-in` iteration calls `next()` on the same initial-state iterator, producing an infinite loop.

## Options Considered

### Option A: Reference semantics for iterators

Allocate iterators on the heap via `Rc<RefCell<Value>>` and let method calls mutate through a shared reference.

**Cons:**
- The evaluator's entire value model is value-semantics. Introducing reference semantics for one case would be inconsistent and would require tracking which values are reference-typed.
- Contradicts the design goal: value semantics are correct for Moonlane even if the PoC doesn't fully achieve them yet (see RFC-0006).

### Option B: Desugar `for-in` to an explicit `mut` variable

Before the loop body, bind the iterable to a `mut` variable. Each `next()` call goes through `env.set()` to update that variable.

**Cons:**
- Requires the parser or construction pass to inject a synthetic `mut` declaration, which complicates the AST/typed-AST boundary.
- The for-in construct already has a clear structure; modifying it for desugaring adds invisible complexity.

### Option C: Evaluator-side writeback after each `next()` call

When `eval_for_in` calls the iterator's `next()` method and receives the mutated iterator as part of the return (via a special `Signal::MutSelfReturn` or by convention), write the mutated iterator back to the environment variable before continuing to the next iteration.

The evaluator inspects `ClosureValue.params` to detect whether the first param is `mut self` and, after the call, extracts the mutated `self` from the closure's parameter scope before it is discarded.

**Pros:**
- No AST changes required
- Consistent with value semantics: the write-back is explicit and local to `eval_for_in`
- Simple to understand: "after calling `next()`, the iterator in the env is updated to the post-call value"

## Decision

**Option C — evaluator-side writeback.**

`eval_for_in` calls the `next()` closure, then checks whether its first parameter was `mut self`; if so, it reads the parameter's current value from the closure's execution environment and writes it back to the iterator binding in the outer env via `env.set()`.

## Consequences

- Any future aspect method taking `mut self` in a loop context will need similar writeback logic, or a more general `mut self` propagation mechanism must be designed.
- This is a known limitation: calling a `mut self` method outside a `for-in` (e.g., `counter.next()` as a standalone call) does not writeback to `counter` in the caller's scope. The caller must reassign: `counter = counter.next_and_return_self()`. Tracked as a known limitation in `evaluator.md`.

## References

- #11 — Iterable<T> aspect and for-in upgrade
- [ADR-0006](adr-0006-evaluator-runtime-design.md) — Evaluator runtime design (value semantics)
- RFC-0006 — Closure capture semantics (future proper fix for reference semantics)
