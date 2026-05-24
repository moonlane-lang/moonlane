---
id: rfc-0019
title: "Return Context Type Propagation"
date: '2026-05-22'
status: incorporated
---

## Summary

Fix a bug in typechecker Pass 2 where `return` and `break` statements do not propagate the enclosing function's return type as the expected type for the return value. This causes type inference to fail on any under-constrained expression in return position — most visibly `return nope;` and `return Result::Err { ... };` — even when the function's return type annotation fully determines the missing type.

---

## Motivation

The spec states that `nope`'s type must be "determinable from context" and shows `return Result::Err { ... }` as valid code in a function annotated `-> Result<Float, String>`. Both currently fail with E0002 at the typechecker's Pass 2 stage.

```moonlane
fun find(arr: Int[], target: Int) -> Perhaps<Int> {
    return nope;            // E0002 — cannot infer type of `nope`
}

fun divide(a: Float, b: Float) -> Result<Float, String> {
    return Result::Err { error: "division by zero" };   // E0002 — cannot infer type
    return Result::Ok { value: a / b };                 // fine — T resolved from field
}
```

The same failure occurs with `break nope;` and `break Result::Err { ... };` inside a `loop`.

The same expressions work correctly as tail expressions (last line of a function body without `return`) because `construct_block` already receives the function's return type and propagates it. Only the explicit `return` and `break` paths are broken.

---

## Root Cause

Pass 2 (`construction.rs`) builds a `ConstructCtx` that has no field tracking the current function's return type. `construct_stmt` therefore passes `None` as `expected_ty` when constructing the return value:

```rust
Stmt::Return(r) => {
    let value = match &r.value {
        Some(e) => Some(construct_expr(e, None, ctx)?),   // <-- None
        None    => None,
    };
```

`construct_expr(nope, None)` reaches `construct_literal_type(Nope, None, span)` which requires `expected_ty` and fails. `construct_enum_literal_ty` for a fieldless or partially-constrained variant (e.g. `Result::Err` where `T` is absent from the variant's fields) also requires `expected_ty` to fill in the unresolved type parameter.

Pass 1 (`inference.rs`) does not have this bug — `InferContext` has `current_return_type` and `construct_stmt` in inference correctly adds a constraint between the return value's type and the current return type.

---

## Proposal

### Pass 2 — `ConstructCtx`

Add a `current_return_ty` field and push/pop methods mirroring what `InferContext` already does:

```rust
struct ConstructCtx<'a> {
    // ... existing fields ...
    current_return_ty: Option<Type>,
}

impl<'a> ConstructCtx<'a> {
    fn push_return_type(&mut self, ty: Option<Type>) -> Option<Type> {
        std::mem::replace(&mut self.current_return_ty, ty)
    }
    fn pop_return_type(&mut self, saved: Option<Type>) {
        self.current_return_ty = saved;
    }
}
```

### `construct_fun_decl` and `construct_method_decl`

Push the resolved return type before constructing the body, pop after:

```rust
let saved = ctx.push_return_type(ret_ty.clone());
let typed_block = construct_block(&fun.body, ret_ty.as_ref(), ctx)?;
ctx.pop_return_type(saved);
```

### `construct_stmt`

For `Return` and `Break`, pass `ctx.current_return_ty.as_ref()` as `expected_ty`:

```rust
Stmt::Return(r) => {
    let value = match &r.value {
        Some(e) => Some(construct_expr(e, ctx.current_return_ty.as_ref(), ctx)?),
        None    => None,
    };
```

For `Break`, the expected type is the loop's break type, not the return type. A `current_break_ty` field follows the same pattern and is pushed when entering a `loop` body and popped on exit. Since `break` with a value only occurs inside `loop`, this is a separate stack entry.

### No spec changes required

The spec already correctly describes this behaviour. The fix makes the implementation match the spec.

---

## Alternatives Considered

**Store a type variable per `nope` literal in the AST (annotate during Pass 1) and resolve it in Pass 2 via the substitution.** This would avoid the need for `expected_ty` entirely for `nope`. It would require the AST to carry inference metadata (type variable IDs), which is a larger structural change and inconsistent with the current two-pass design where the AST is untyped. The `expected_ty` propagation approach is consistent with how empty arrays (`[]`) and other under-constrained literals are already handled.

---

## Open Questions

- Should `construct_stmt(Break)` use a dedicated `current_break_ty` (pushed when entering `loop`), or should it fall back to `current_return_ty`? Using a dedicated stack is cleaner and mirrors the inference pass design. There is no case where a `break` value should inherit the function's return type — they are independent.

---

## Timing Recommendation

Small, isolated fix with no design risk. No new language surface. Implement before any v0.2 work that exercises `Perhaps<T>` or `Result<T, E>` in function return position.

---

## References

- Language spec: `docs/public/spec/types.md` (§ `Perhaps<T>`, § `Result<T, E>`)
- `moonlane-interpreter/src/typechecker/construction.rs`
- `moonlane-interpreter/src/typechecker/inference.rs` — `InferContext::current_return_type` for reference

## Decision

**Outcome:** Accepted
**Target:** v0.2

Pure bug fix — no design question, implementation matches existing Pass 1 pattern exactly.
