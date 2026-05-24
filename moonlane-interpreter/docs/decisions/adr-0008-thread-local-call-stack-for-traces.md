# ADR-0008 — Thread-Local Call Stack for Runtime Error Traces

**Date:** 2026-05-23  
**Status:** Accepted

---

## Context

Runtime panics should display a call-stack trace (innermost function first) so users can identify where an error originated. The evaluator needed a place to accumulate `FrameInfo { fn_name, call_site }` records as functions are called and unwound.

Two plausible approaches:

1. **Thread-local storage** — A `thread_local! { static CALL_STACK: RefCell<Vec<FrameInfo>> }` that every `call_function` pushes to and pops from.
2. **Explicit parameter** — Thread a `&mut Vec<FrameInfo>` through every `eval_expr`, `eval_block`, `eval_decl`, and `call_function` call.

---

## Decision

**Thread-local storage.** `CALL_STACK` is a `thread_local!` `RefCell<Vec<FrameInfo>>` in `src/evaluator/mod.rs`.

---

## Rationale

Adding a stack parameter to every eval function would require changing every function signature and every call site across the evaluator — high churn for a PoC feature. Thread-local is invisible to the call graph and achieves the same result because the evaluator is single-threaded in v0.1.

---

## Consequences

- `CALL_STACK` must be cleared at the start of each `evaluate()` call, or traces from a previous program appear in the next one.
- `main()` is not pushed: it is executed directly via `eval_block` (not `call_function`), so it never appears in traces. This is intentional — see `evaluator.md`.
- Anonymous closures are pushed as `<closure>` since they have no name in the AST.
- If the evaluator is ever made multi-threaded, each thread already has its own `CALL_STACK` (thread-local by construction), which is correct.
- If a future rewrite passes the stack explicitly, the thread-local can be removed without changing user-visible behaviour.
