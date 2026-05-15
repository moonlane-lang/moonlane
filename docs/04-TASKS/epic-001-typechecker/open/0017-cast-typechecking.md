# Task 0017: Type Cast (`as`) Typechecking

**Status:** open
**Epic:** epic-001-typechecker
**Component:** typechecker
**Spec Link:** `../01-SPEC/LANGUAGE-SPEC.md` §3.5 Type casting (`as`)
**Blocked By:** none

## What

`Expr::Cast` falls through to `_ => internal error` in `infer_expr`. The `as`
operator is the only in-language mechanism for numeric type conversion.

```rust
Expr::Cast { expr: Box<Expr>, target_type: TypeExpr, span }
```

## Typing Rules

The spec defines `as` for primitive numeric casts: `Int ↔ Float`.

```rust
Expr::Cast { expr, target_type, span } => {
    let source_ty = infer_expr(expr, ctx, fun_generalizations)?;
    let target_ty = type_expr_to_infer(target_type);
    // Both source and target must be numeric primitives.
    // Emit constraints: source_ty == one of {Int, Float},
    //                   target_ty == one of {Int, Float}.
    // Simplest correct approach: just return target_ty unconditionally
    // and let the evaluator enforce the runtime semantics.
    // Stricter: require source to be numeric, return target as-is.
    let num_var = ctx.fresh_var();
    ctx.add_constraint(source_ty, num_var.clone(), span.clone());
    // Constrain num_var to be numeric by unifying with a fresh result;
    // the evaluator will panic at runtime if the cast is invalid.
    // For v0.1, accept any source type and trust the annotation.
    Ok(target_ty)
}
```

**Stricter alternative** (recommended): check that source is `Int` or `Float`
and target is `Int` or `Float`. This catches obvious errors like `"hello" as Float`:

```rust
Expr::Cast { expr, target_type, span } => {
    let source_ty = infer_expr(expr, ctx, fun_generalizations)?;
    let target_ty  = type_expr_to_infer(target_type);
    // Verify source is numeric: constrain source_ty to a fresh var, then
    // verify after solving. Alternatively, emit constraint via a sentinel.
    // Simplest correct approach for v0.1: constrain source to be either
    // Int or Float by checking it unifies with the target's numeric family.
    // Both sides must be numeric — use a shared fresh variable:
    let num_var = ctx.fresh_var();
    ctx.add_constraint(source_ty, num_var.clone(), span.clone());
    ctx.add_constraint(target_ty.clone(), num_var, span.clone());
    Ok(target_ty)
}
```

Note: using a single fresh variable for both sides enforces that source and
target are in the same numeric family but does not prevent `Int as Int`. That
is fine — identity casts are harmless.

For non-numeric casts (`String as Int`): the constraint `String == Float` (or
`String == Int`) will fail with E0001 at solve time.

> **Provisional:** the numeric-family constraint is a v0.1 simplification.
> The spec (§3.5) states that `as` desugars to the `From` trait, making
> user-defined casts possible via `impl From<S> for T`. Epic 004 task 0002
> replaces this check with a proper `From<S>` trait lookup. At that point
> `Int as Float` dispatches through a built-in `impl From<Int> for Float`,
> and any non-numeric source type that does not implement the required `From`
> impl produces E0001 via the trait resolver rather than the constraint solver.

## Pass 2

```rust
Expr::Cast { expr, target_type, span } => {
    let typed_expr = construct_expr(expr, None, ctx)?;
    let ty = resolved_to_type(&type_expr_to_infer(target_type), ctx.subst, span)?;
    Ok(TypedExpr::Cast { expr: Box::new(typed_expr), target_type: target_type.clone(), ty, span: span.clone() })
}
```

## Acceptance Criteria

- [ ] `Expr::Cast` is handled in `infer_expr`; result type is the target type
- [ ] `Int as Float` and `Float as Int` type-check correctly
- [ ] Non-numeric cast (e.g. `"hello" as Int`) → E0001 (provisional — see Epic 004 task 0002)
- [ ] Pass 2 constructs `TypedExpr::Cast` with the target type
- [ ] Positive test: `let f: Float = x as Float` where `x: Int`
- [ ] Positive test: `let i: Int = f as Int` where `f: Float`
- [ ] Negative test: casting a `String` to a numeric type → E0001
- [ ] All prior tests still pass
