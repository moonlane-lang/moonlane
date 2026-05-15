# Task 0020: `Expr::Path` Typechecking

**Status:** open
**Epic:** epic-001-typechecker
**Component:** typechecker
**Spec Link:** `../01-SPEC/LANGUAGE-SPEC.md` §5.1 Associated functions, §7.1 Enum instantiation
**Blocked By:** 0019 (needs enum_env for unit variant lookup)

## What

`Expr::Path(Vec<String>, Span)` falls through to `_ => internal error`. Two distinct uses:

1. **Unit enum variant values**: `Direction::North` — the path expression itself is the value.
2. **Associated function references**: `Point::new(1.0, 2.0)` parses as
   `Expr::Call { callee: Expr::Path(["Point", "new"]), args: [...] }`. The path must
   resolve to the method's function type so the surrounding `Expr::Call` can typecheck it.

Both are two-segment paths `[TypeName, MemberName]`. Only two-segment paths are
handled here — longer paths require module resolution, which is explicitly deferred
(see spec backlog: "Module system / use"). The `else` branch in the typing rules
returns E0003 for any path that is not exactly two segments; this is the correct
v0.1 behaviour and will be replaced by a module resolver when that epic is implemented.

## Typing Rules

```rust
Expr::Path(segments, span) => {
    let [type_name, member_name] = segments.as_slice() else {
        return Err(YoloscriptError::type_error(
            ErrorCode::E0003,
            format!("unresolved path `{}`", segments.join("::")),
            span,
        ));
    };

    // Associated function or non-self method in method_env?
    if let Some(fun_ty) = ctx.get_method_type(type_name, member_name).cloned() {
        return Ok(fun_ty);
    }

    // Unit enum variant (no fields)?
    if let Some(variants) = ctx.get_enum_variants(type_name) {
        if variants.iter().any(|v| v.name == *member_name && v.fields.is_empty()) {
            return Ok(InferType::Named(type_name.clone(), vec![]));
        }
    }

    Err(YoloscriptError::type_error(
        ErrorCode::E0003,
        format!("no member `{member_name}` on type `{type_name}`"),
        span,
    ))
}
```

### Associated function call mechanics

When `Expr::Path` resolves to a type from `method_env`, the stored type is
`Fun(params, ret)`. For instance methods, `params[0]` is the `self` type. For
associated functions (no `self` parameter), `params` starts with the first
explicit parameter. The surrounding `Expr::Call` unification handles both
cases generically — no special-casing needed in the path handler.

### Non-unit variants

Struct-like enum variants (`Shape::Circle { radius }`) are handled by
`Expr::StructLiteral` (task 0019), not `Expr::Path`. A path that names a non-unit
variant will not match the `v.fields.is_empty()` guard and will fall through to E0003,
which is correct — you cannot use a struct variant as a bare value.

## Pass 2

```rust
Expr::Path(segments, span) => {
    let [type_name, member_name] = segments.as_slice() else {
        return Err(YoloscriptError::internal("invalid path in construct"));
    };
    // Associated function?
    if let Some(ty) = ctx.method_env
        .get(type_name.as_str())
        .and_then(|m| m.get(member_name.as_str()))
        .cloned()
    {
        return Ok(TypedExpr::Path(segments.clone(), ty, span.clone()));
    }
    // Unit enum variant — type is Named(EnumName, []).
    Ok(TypedExpr::Path(
        segments.clone(),
        Type::Named(type_name.clone(), vec![]),
        span.clone(),
    ))
}
```

## Notes

### Module resolution is deferred

The spec backlog explicitly defers the module system (`use`, visibility, multi-file
programs). When it is implemented, `Expr::Path` with three or more segments will
represent module-qualified names such as `math::trig::sin` or
`my_module::MyType::associated_fn`. The module resolver will need to:

1. Walk the leading segments as a module path.
2. Resolve the final one or two segments as a type member within that module.

The current implementation intentionally returns E0003 for any path that is not
exactly two segments, making the deferred boundary explicit and crash-free. When
the module system epic is implemented, this function is the correct extension point.

## Acceptance Criteria

- [ ] `Expr::Path` with two segments resolves to associated function type or unit variant type
- [ ] `Point::new(1.0, 2.0)` typechecks when `new` is registered in `method_env`
- [ ] `Direction::North` produces `Named("Direction", [])` type
- [ ] Path naming a struct-like (non-unit) variant → E0003
- [ ] Unrecognised member on known type → E0003
- [ ] Pass 2 constructs `TypedExpr::Path` with the correct type
- [ ] Positive test: associated function call via `Type::fun_name(args)`
- [ ] Positive test: unit enum variant used as a value
- [ ] Negative test: unrecognised path → E0003
- [ ] All prior tests still pass
