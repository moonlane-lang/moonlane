# Task 0003: `From`-Based Error Coercion in `?`

**Status:** open
**Epic:** epic-004-traits
**Component:** typechecker, evaluator
**Spec Link:** `../01-SPEC/LANGUAGE-SPEC.md` §5.4 The `?` operator, §10 Traits
**Spec Backlog:** `? error type coercion` — explicitly deferred from v0.1
**Blocked By:** Epic 004 trait infrastructure; task 0002 (`From` trait definition)

## What

Epic 001 task 0018 implements `?` with exact error type matching: the inner
expression's error type `E` and the enclosing function's return error type are
constrained to the same fresh type variable. If they differ at all, E0001 fires.

This is correct for v0.1 but too restrictive in practice. In Rust (and in the
spec's intent), `?` automatically coerces the error type by calling `From::from(e)`
when the inner error type `E1` differs from the function's expected error type `E2`,
provided `E2: From<E1>`.

This task upgrades `?` to use `From`-based coercion.

## Design

### Semantics

`expr?` in a function returning `Result<_, E2>`:

1. If `expr: Result<T, E1>` and `E1 == E2`: propagate `Err(e)` unchanged.
2. If `expr: Result<T, E1>` and `E2: From<E1>`: propagate `Err(E2::from(e))`.
3. Otherwise: type error — `E2` does not implement `From<E1>`.

### Typechecking change

Replace the shared `err_var` constraint with a `From` requirement:

```rust
Expr::PropagateError { expr, span } => {
    let inner_ty = infer_expr(expr, ctx, fun_generalizations)?;
    let ok_var  = ctx.fresh_var();
    let err_var = ctx.fresh_var();  // E1: inner error type
    ctx.add_constraint(
        inner_ty,
        InferType::Named("Result".to_string(), vec![ok_var.clone(), err_var.clone()]),
        span.clone(),
    );

    if let Some(fn_ret) = ctx.current_return_type().cloned() {
        let fn_ok_var  = ctx.fresh_var();
        let fn_err_var = ctx.fresh_var();  // E2: function's error type
        ctx.add_constraint(
            fn_ret,
            InferType::Named("Result".to_string(), vec![fn_ok_var, fn_err_var.clone()]),
            span.clone(),
        );
        // Require E2: From<E1> instead of E1 == E2.
        ctx.require_trait_impl_deferred(&err_var, &fn_err_var, "From", span.clone());
        // require_trait_impl_deferred records the constraint for post-solve checking,
        // since the types may still be variables at this point.
    }

    Ok(ok_var)
}
```

### Post-solve trait check

Because `err_var` and `fn_err_var` may still be unresolved type variables when
the constraint is emitted, the `From` check cannot be done eagerly. Two options:

**A)** Resolve after `ctx.solve()` in `check()`: walk the deferred `From` constraints,
apply the final substitution to both sides, then verify the impl exists.

**B)** Emit a sentinel constraint type (`InferType::TraitBound(...)`) and handle it
in the solver as a post-processing step.

Option A is simpler for v0.1 of this task.

### Evaluator change

When `E1 != E2` but `E2: From<E1>`, the evaluator must insert a call to
`E2::from(e)` before re-wrapping in `Err`. This requires the typed AST to record
whether a coercion is needed. Add an optional field to `TypedExpr::PropagateError`:

```rust
PropagateError {
    expr:     Box<TypedExpr>,
    coercion: Option<Box<TypedExpr>>,  // Some(from_fn) if E1 != E2
    ty:       Type,
    span:     Span,
}
```

The evaluator calls `coercion(e)` on the extracted error value before propagating.

## Acceptance Criteria

- [ ] `?` with matching error types still works (no coercion, no regression)
- [ ] `?` with `E2: From<E1>` coerces automatically — no user annotation needed
- [ ] `?` with incompatible error types (no `From` impl) → E0001
- [ ] Post-solve `From` check resolves type variables before verifying the impl
- [ ] Typed AST records the coercion function when types differ
- [ ] Evaluator calls the coercion function before propagating `Err`
- [ ] Positive test: `?` propagating `E1` into a function returning `Result<_, E2>`
  where `impl From<E1> for E2` exists
- [ ] Negative test: `?` with no `From` impl → E0001
- [ ] All Epic 001 `?` tests still pass

## Notes

This task depends on task 0002 (`From` trait definition and built-in numeric impls)
since it reuses the `require_trait_impl` machinery and the `From` trait registration
mechanism.

Once done, remove the `> Provisional:` note from Epic 001 task 0018 and update its
Notes section accordingly.
