# Task 0016: Closure Typechecking

**Status:** open
**Epic:** epic-001-typechecker
**Component:** typechecker
**Spec Link:** `../01-SPEC/LANGUAGE-SPEC.md` §5.3 Closures
**Blocked By:** none

## What

`Expr::Closure` falls through to `_ => internal error` in `infer_expr`. Closures
are first-class values; their type is `Fun(param_types, ret_type)`.

```rust
pub struct Closure in Expr {
    params:      Vec<Param>,
    return_type: Option<TypeExpr>,
    body:        Block,
}
```

## Typing Rules

Closures are essentially anonymous functions. The inference logic mirrors
`infer_fun_decl` minus the name-registration step:

1. **Param types**: use annotation if present, otherwise a fresh variable (same as functions).
2. **Return type**: use annotation if present, otherwise a fresh variable.
3. **Capture**: do **not** push a new isolated scope — the closure body shares the
   enclosing `mono_env` scope stack (closures capture by lexical scope). Push one
   new scope for the params on top of the existing stack.
4. **Body**: infer in that scope. Constrain body type == return type.
5. **Pop** the param scope.
6. **Result**: `InferType::Fun(param_types, Box::new(ret_ty))`.

Do **not** run the inline partial solve or call `bind_poly` — closures are
expressions, not top-level declarations. They don't get generalized into schemes.

### Note on capture semantics

The spec says "captured `mut` variables are shared — mutations are visible in the
outer scope." The typechecker only needs to verify types; the evaluator handles
the sharing semantics. For typechecking, the closure body simply sees all variables
in scope at the point of definition.

## Pass 2

```rust
Expr::Closure { params, return_type, body, span } => {
    let param_types: Vec<Type> = params.iter()
        .map(|p| p.type_ann.as_ref()
            .map(|ann| resolved_to_type(&type_expr_to_infer(ann), ctx.subst, &p.span))
            .unwrap_or_else(|| Err(YoloscriptError::type_error(
                ErrorCode::E0002,
                format!("closure parameter `{}` needs a type annotation", p.name),
                &p.span,
            ))))
        .collect::<Result<_, _>>()?;
    let ret_ty = return_type.as_ref()
        .map(|ann| resolved_to_type(&type_expr_to_infer(ann), ctx.subst, span))
        .transpose()?
        .unwrap_or(Type::Unit);
    ctx.push_scope();
    for (p, ty) in params.iter().zip(param_types.iter()) {
        ctx.bind(&p.name, ty.clone());
    }
    let typed_body = construct_block(body, ctx)?;
    ctx.pop_scope();
    let ty = Type::Fun(param_types, Box::new(ret_ty));
    Ok(TypedExpr::Closure { params: params.clone(), return_type: return_type.clone(), body: typed_body, ty, span: span.clone() })
}
```

## Acceptance Criteria

- [ ] `Expr::Closure` is handled in `infer_expr`; produces `Fun(params, ret)` type
- [ ] Closure body sees variables from the enclosing scope (capture works)
- [ ] Closure params with annotations use the declared type
- [ ] Closure params without annotations use fresh type variables
- [ ] Body type is constrained against return type (annotation or fresh var)
- [ ] Pass 2 constructs `TypedExpr::Closure` with a typed body
- [ ] Positive test: closure passed to a higher-order function; result type inferred
- [ ] Positive test: closure captures an outer variable
- [ ] Positive test: closure with explicit param/return type annotations
- [ ] Negative test: body type mismatch with return annotation → E0001
- [ ] All prior tests still pass
