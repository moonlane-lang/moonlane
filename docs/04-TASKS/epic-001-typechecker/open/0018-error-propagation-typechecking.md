# Task 0018: Error Propagation (`?`) Typechecking

**Status:** open
**Epic:** epic-001-typechecker
**Component:** typechecker
**Spec Link:** `../01-SPEC/LANGUAGE-SPEC.md` §5.4 The `?` operator
**Blocked By:** 0014 (match expression — needed to use `Result` values without `?`; not a hard block but `Result` testing is cleaner with match)

## What

`Expr::PropagateError` falls through to `_ => internal error` in `infer_expr`.
The `?` operator is the ergonomic way to handle `Result<T, E>` — without it,
every fallible call requires a `match`.

```rust
Expr::PropagateError { expr: Box<Expr>, span }
```

The spec describes `?` as:

> If the expression is `Err(e)`, return `Err(e)` immediately; otherwise unwrap
> to the `Ok` value. Error types must match exactly — no implicit coercion.

## Typing Rules

Let `inner_ty = infer_expr(expr)`. The inner expression must be `Result<T, E>`:

```rust
Expr::PropagateError { expr, span } => {
    let inner_ty = infer_expr(expr, ctx, fun_generalizations)?;
    let ok_var  = ctx.fresh_var();
    let err_var = ctx.fresh_var();
    // inner_ty must be Result<T, E>
    ctx.add_constraint(
        inner_ty,
        InferType::Named("Result".to_string(), vec![ok_var.clone(), err_var.clone()]),
        span.clone(),
    );
    // The enclosing function's return type must be Result<_, E>
    // (same error type, any Ok type).
    if let Some(fn_ret) = ctx.current_return_type().cloned() {
        let fn_ok_var = ctx.fresh_var();
        ctx.add_constraint(
            fn_ret,
            InferType::Named("Result".to_string(), vec![fn_ok_var, err_var]),
            span.clone(),
        );
    }
    // The expression produces the unwrapped Ok value.
    Ok(ok_var)
}
```

> **Provisional:** sharing a single `err_var` across the inner expression and the
> function return type enforces exact error type equality. The spec backlog explicitly
> defers `From`-based coercion to after v0.1: Epic 004 task 0003 replaces the shared
> variable with a `From` constraint — the function's error type `E2` must implement
> `From<E1>`, and `?` automatically inserts a `From::from(e)` call when `E1 != E2`.
> No change to this task is needed until then.

```rust
```

If there is no enclosing function return type (top-level `?`), the constraint on
`fn_ret` is skipped. This mirrors how `return` works outside a function — it's
technically invalid but caught at a later stage or by the evaluator.

### `InferType::Named("Result", ...)` vs `InferType::Concrete(Type::Result(...))`

`Result<T, E>` is represented as `InferType::Named("Result", [T, E])` during
inference, matching the convention used for `Perhaps<T>`. The constraint solver
handles `Named` unification generically.

## Pass 2

```rust
Expr::PropagateError { expr, span } => {
    let typed_expr = construct_expr(expr, None, ctx)?;
    let ty = match typed_expr.ty() {
        Type::Result(ok, _) => *ok.clone(),
        _ => return Err(YoloscriptError::internal("? on non-Result value")),
    };
    Ok(TypedExpr::PropagateError { expr: Box::new(typed_expr), ty, span: span.clone() })
}
```

## Acceptance Criteria

- [ ] `Expr::PropagateError` constrains inner expression to `Result<T, E>`
- [ ] Returns the unwrapped `T` type
- [ ] Enclosing function return type is constrained to `Result<_, E>` with matching error type
- [ ] Pass 2 constructs `TypedExpr::PropagateError` with the `Ok` type
- [ ] Positive test: `let x = fallible()?` inside a `Result`-returning function
- [ ] Negative test: `?` applied to a non-`Result` value → E0001
- [ ] All prior tests still pass

## Notes

The spec states error types must match exactly (no coercion) for v0.1. This is
enforced naturally because both the `?` expression's `E` and the function's return
`E` are constrained to the same fresh type variable — if they differ, E0001 fires.

The spec backlog records `? error type coercion` as explicitly deferred: Epic 004
task 0003 upgrades `?` to allow `From`-based coercion between compatible error types.
