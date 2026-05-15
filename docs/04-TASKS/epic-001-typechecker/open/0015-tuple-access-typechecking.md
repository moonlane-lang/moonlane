# Task 0015: Tuple Access Typechecking

**Status:** open
**Epic:** epic-001-typechecker
**Component:** typechecker
**Spec Link:** `../01-SPEC/LANGUAGE-SPEC.md` §3.3 Tuples
**Blocked By:** none

## What

`Expr::TupleAccess` falls through to `_ => internal error` in both `infer_expr`
and `construct_expr`. Tuple construction (`Expr::Tuple`) is already typechecked
and produces `InferType::Tuple(elem_tys)`. Accessing `.0`, `.1`, etc. is the
missing half.

## Typing Rules

```rust
Expr::TupleAccess { object, index, span } => {
    let obj_ty = infer_expr(object, ctx, fun_generalizations)?;
    let obj_ty = ctx.solve()?.apply(&obj_ty);  // resolve through variables
    match &obj_ty {
        InferType::Tuple(elems) => {
            elems.get(*index).cloned().ok_or_else(|| YoloscriptError::type_error(
                ErrorCode::E0003,
                format!("tuple index {index} out of bounds (tuple has {} elements)", elems.len()),
                span,
            ))
        }
        _ => Err(YoloscriptError::type_error(
            ErrorCode::E0002,
            "cannot infer tuple type for index access; add a type annotation",
            span,
        )),
    }
}
```

The partial-solve step (`ctx.solve()?.apply(...)`) is the same pattern used in
`FieldAccess` and `MethodCall` so that type variables flowing through let-bindings
are resolved before the index lookup.

## Pass 2

```rust
Expr::TupleAccess { object, index, span } => {
    let typed_obj = construct_expr(object, None, ctx)?;
    let ty = match typed_obj.ty() {
        Type::Tuple(elems) => elems.get(*index).cloned()
            .ok_or_else(|| YoloscriptError::internal(
                format!("tuple index {index} out of bounds")
            ))?,
        _ => return Err(YoloscriptError::internal("tuple access on non-tuple")),
    };
    Ok(TypedExpr::TupleAccess { object: Box::new(typed_obj), index: *index, ty, span: span.clone() })
}
```

## Acceptance Criteria

- [ ] `Expr::TupleAccess` is handled in `infer_expr`; object type resolved via partial solve
- [ ] Returns the type of the element at `index`
- [ ] Out-of-bounds index → E0003
- [ ] Non-tuple object → E0002
- [ ] Pass 2 constructs `TypedExpr::TupleAccess` correctly
- [ ] Positive test: access `.0` and `.1` on a tuple value
- [ ] Positive test: tuple returned from a function, then indexed
- [ ] Negative test: out-of-bounds index → E0003
- [ ] All prior tests still pass
