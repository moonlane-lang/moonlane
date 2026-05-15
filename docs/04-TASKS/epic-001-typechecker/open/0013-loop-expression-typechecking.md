# Task 0013: `loop` Expression Typechecking

**Status:** open
**Epic:** epic-001-typechecker
**Component:** typechecker
**Spec Link:** `../01-SPEC/LANGUAGE-SPEC.md` §12.5 Loop, §12.6 Break and continue, §3.7 Never type
**Blocked By:** none

## What

`Expr::Loop` falls through to `_ => internal error` in `infer_expr`. Additionally,
`Stmt::Break` currently infers the break value expression (if any) but does not
constrain it against the enclosing loop's expected type — it just returns `Never`.
These two must be fixed together.

## Typing Rules

### `Expr::Loop`

A `loop` body is a block. Two cases:

1. **No reachable `break`**: type is `Never` (`!`).
2. **One or more `break expr`**: type is `T` where every `break expr` has type `T`.
   All break values must unify.

The implementation approach:

- Introduce a **loop break type variable** `break_var = ctx.fresh_var()` before
  inferring the body.
- Push `break_var` onto a break-type stack on `ctx` (similar to `push_return_type`).
- During body inference, `Stmt::Break { value: Some(e) }` constrains `e`'s type
  to unify with the current loop's `break_var`.
- `Stmt::Break { value: None }` constrains `break_var == Unit` (a bare `break`
  means the loop produces `Unit`).
- After the body, pop the break type. The loop's type is `break_var` (which may
  remain a fresh var — unifying with `Never` — if there are no `break` arms).

### `Stmt::Break` update

Currently `infer_stmt` for `Break` infers the value but does not use it:

```rust
Stmt::Break(bs) => {
    if let Some(e) = &bs.value {
        infer_expr(e, ctx, fun_generalizations)?;
    }
    Ok(InferType::never())
}
```

Update to emit a constraint against the current loop break type:

```rust
Stmt::Break(bs) => {
    let break_ty = match &bs.value {
        Some(e) => infer_expr(e, ctx, fun_generalizations)?,
        None    => InferType::unit(),
    };
    if let Some(expected) = ctx.current_break_type().cloned() {
        ctx.add_constraint(break_ty, expected, bs.span.clone());
    }
    Ok(InferType::never())
}
```

### New `InferContext` state

Mirror the `push_return_type` / `pop_return_type` pattern:

```rust
push_break_type(ty: InferType) -> Option<InferType>
pop_break_type(prev: Option<InferType>)
current_break_type() -> Option<&InferType>
```

## Pass 2

`construct_expr` for `Expr::Loop`: construct the body block, determine the
concrete loop type from the substitution-applied break variable. If the variable
is still unresolved (no break), the type is `Never`.

`construct_stmt` for `Stmt::Break`: construct the break value expression if
present. The `TypedBreakStmt` already exists in `typed_ast`.

## Acceptance Criteria

- [ ] `InferContext` has `push_break_type` / `pop_break_type` / `current_break_type`
- [ ] `Expr::Loop` infers the body; result type is the break type variable
- [ ] `loop { }` (no break) resolves to `Never`
- [ ] `loop { break 42; }` resolves to `Int`
- [ ] All `break expr` arms in one loop must unify — mismatch → E0001
- [ ] `Stmt::Break` constrains the break value against the loop break type
- [ ] Pass 2 constructs `TypedExpr::Loop` and `TypedStmt::Break` correctly
- [ ] Positive test: `let x: Int = loop { break 42; }` type-checks
- [ ] Positive test: infinite loop (`loop { }`) with type `Never` in a diverging context
- [ ] Negative test: mismatched break value types → E0001
- [ ] All prior tests still pass
