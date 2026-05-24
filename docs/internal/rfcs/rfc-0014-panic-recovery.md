---
id: rfc-0014
title: "Panic Recovery"
date: '2026-05-21'
status: draft
---

## Summary

Define whether and how Moonlane programs can recover from panics — the runtime errors currently triggered by `.yolo()` on `nope`/`Err`, out-of-bounds access, and division by zero.

---

## Motivation

The spec ([Panics](../../public/spec/runtime.md#panics)) defines panics as hard, unrecoverable, and uncatchable. This is a deliberate simplicity choice for v0.1. The question is whether this should remain true permanently, or whether a recovery mechanism is needed for production use cases (e.g. server processes that must not crash on a single bad request).

---

## Open Questions

- **Should panic recovery exist at all?** The `Result<T, E>` / `?` pattern is the idiomatic way to handle expected errors. Panic recovery would be for truly unexpected failures. Is it worth the complexity?
- **If yes — semantics**: a `catch` expression? A top-level handler? A fiber-level boundary (panics are contained to the fiber they occur in)?
- **Interaction with `Result`**: does recovery produce a `Result<T, PanicError>` or use a different mechanism?
- **Stack unwinding**: does the runtime unwind the stack on panic, or is recovery only possible at a well-defined boundary?
- **Relationship to concurrency**: RFC-0003 specifies that a fiber panic terminates the program. Does panic recovery change this?

---

## Decision

**Outcome:** *(pending)*  
**Target:** *(blank until accepted)*

*(Decision rationale goes here when the RFC is evaluated.)*
