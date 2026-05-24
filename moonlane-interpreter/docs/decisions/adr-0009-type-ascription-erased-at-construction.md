# ADR-0009 — Type Ascription Erased at Construction (No TypedExpr::Ascribe)

**Date:** 2026-05-23  
**Status:** Accepted

---

## Context

The type ascription operator (`e : T`) is a pure inference hint with no runtime effect. The typechecker needed to decide whether to:

1. **Erase at construction** — Handle `Expr::Ascribe` in Pass 1 by emitting a unification constraint and returning the inner type; in Pass 2, resolve the annotation and pass it as the expected-type hint when constructing the inner expression. No `TypedExpr::Ascribe` variant is introduced.
2. **Preserve as a TypedAST node** — Add `TypedExpr::Ascribe { expr, ty }` and propagate it to the evaluator, which would then discard it.

---

## Decision

**Erase at construction.** No `TypedExpr::Ascribe` variant exists.

---

## Rationale

Ascription has no runtime behaviour — keeping it as a node would add an evaluator arm that does nothing except unwrap the inner expression. The TypedAST is the interface between the typechecker and the evaluator; nodes that carry no runtime-relevant information should not cross that boundary.

Pass 2 already supports an `expected_ty` parameter on `construct_expr` for guiding construction in contexts where the expected type is known from the outside (e.g., struct fields, function return types). Passing the resolved annotation as `expected_ty` reuses this existing mechanism at no extra cost.

---

## Consequences

- The evaluator never sees `Ascribe` nodes. No evaluator change is required when ascription is added.
- A future compiler backend also needs no special handling for ascription.
- If a backend ever needs to reconstruct the annotation for documentation or IDE tooling, the information is unavailable after construction. This is acceptable for a compiled language where ascription is a hint, not a runtime assertion.
- **Invariant:** If `Expr::Ascribe` is extended with runtime semantics (e.g., checked casts), a `TypedExpr::Ascribe` variant must be added at that point. Until then, erase.
