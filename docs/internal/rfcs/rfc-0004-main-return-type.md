---
id: rfc-0004
title: "main() return type — should main return Result instead of ()?"
date: '2026-05-21'
status: draft
---

## Summary

Moonlane's `main()` function currently returns `()`. This was a deliberate design
choice: because `?` is only valid inside functions returning `Result<T, E>`, callers
in `main` must handle errors explicitly — either via `match` or via `.yolo()`, which
panics and signals intentional "I accept program termination if this fails" semantics.
This RFC asks whether that design is still right, or whether allowing `main` to return
`Result<(), E>` would be a better trade-off.

---

## Motivation

The current design has an intentional philosophy: in `main`, every fallible call must
be handled visibly. The `.yolo()` escape hatch exists precisely for this: its name
signals that the author knowingly accepts a panic (and therefore a non-zero, unclean
exit) at that call site. This keeps error paths explicit at the highest level of the
program, consistent with Moonlane's principle of explicit error handling.

In practice, however, this creates friction. Most real programs contain many fallible
operations at the top level (reading config, connecting to a database, parsing
arguments). With the current design, each either demands a `match` block or a
`.yolo()`. The result is either verbose match chains or a scattering of
`.yolo()` calls whose intent is not "I accept a panic" but rather "I want this error
to terminate the program cleanly" — a subtle but meaningful distinction.

Rust's experience is instructive: `fn main() -> Result<(), Box<dyn Error>>` was added
in Rust 1.26 precisely because the ergonomic cost of not having it was high. The
`Termination` trait lets the runtime print the error and exit with a non-zero code,
giving programs a clean, informative exit path without boilerplate.

The question is whether the same trade-off applies to Moonlane, and if so, how to
introduce it without undermining the explicit-handling philosophy.

---

## Proposal

*(To be completed — this RFC is open for discussion.)*

Leading candidate: allow `main` to optionally return `Result<(), E>`. When it does,
the runtime unwraps the result: `Ok(())` exits cleanly; `Err(e)` prints a diagnostic
and exits with a non-zero status. `main() -> ()` remains valid for programs that want
explicit control. The `?` operator becomes legal inside a `Result`-returning `main`.

---

## Alternatives Considered

### A. Keep `main() -> ()` (status quo)

All error handling in `main` stays explicit. `.yolo()` is the standard way to say
"terminate on error." Consistent with the current philosophy; no spec change needed.

**Downside:** the ergonomic cost is real. `.yolo()` is semantically ambiguous — its
name suggests recklessness, not intentional process termination. Large programs become
noisy.

### B. Allow `main() -> Result<(), E>` (opt-in)

Programs that want `?` in main declare `fun main() -> Result<(), E>`. Programs that
do not keep `fun main()`. The runtime detects the return type and behaves accordingly.

**Downside:** two valid signatures for `main` adds a small spec and implementation
complexity. The error type `E` is left open (see Open Questions).

### C. Introduce a dedicated `main() -> ()` with a `yolo` block / scope

A scoped construct (e.g. `yolo { ... }` block) propagates any `?`-derived error
inside it as a panic, making the intent explicit at the block level rather than per
call. Keeps `main` returning `()` while allowing `?` within designated "yolo" regions.

**Downside:** novel syntax with no precedent in similar languages; complexity for a
narrow use case.

### D. Make `main` implicitly return `Result`

Silently treat `main` as returning `Result<(), !>` (or `Result<(), String>`), making
`?` always valid inside it, without requiring an explicit annotation.

**Downside:** hides the return type from the programmer; reduces the transparency that
Moonlane values.

---

## Open Questions

1. **Error type for `main`** — if `main` returns `Result<(), E>`, what is `E`? A
   concrete type forces a single error type; a type variable requires inference at the
   declaration site; a built-in "any error" erasure type (like `Box<dyn Error>`) needs
   the trait system (v0.2+).

2. **Runtime output format** — if the runtime exits via `Err(e)`, what gets printed?
   Just `e` via some `Display`-equivalent? A fixed prefix like `"error: "`?

3. **Backward compatibility** — can both `main() -> ()` and `main() -> Result<(), E>`
   coexist, or should `main() -> ()` be deprecated once this is supported?

4. **Relationship to `.yolo()`** — if `main` can return `Result`, does `.yolo()` in
   `main` become an anti-pattern? Does this blur the intended semantics of `.yolo()`?

5. **Exit code** — should an `Err` exit use a fixed code (e.g. 1), or should the
   error value be able to carry an exit code?

---

## Timing Recommendation

This is a **spec-level design question** that should be resolved before the compiler
backend is implemented. It does not block v0.1 interpreter work — the evaluator
already panics on unexpected signals from `main` — but it should be decided before
v0.2 feature work begins to avoid a breaking change later.

---

## References

- Language spec: [`spec/functions.md`](../../public/spec/functions.md), [`spec/types.md#resultt-e`](../../public/spec/types.md#resultt-e), [`spec/runtime.md#panics`](../../public/spec/runtime.md#panics)
- Spec backlog: `docs/internal/spec-backlog.md` — unwrap syntax entry
- Rust RFC 1937: `fn main()` returning `Result` (`std::process::Termination`)

---

## Decision

**Outcome:** *(pending)*  
**Target:** *(blank until accepted)*

*(Decision rationale goes here when the RFC is evaluated.)*
