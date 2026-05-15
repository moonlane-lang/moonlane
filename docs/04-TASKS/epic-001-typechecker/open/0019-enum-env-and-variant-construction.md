# Task 0019: Enum Environment and Variant Construction

**Status:** open
**Epic:** epic-001-typechecker
**Component:** typechecker
**Spec Link:** `../01-SPEC/LANGUAGE-SPEC.md` §7 Enums, §8 Perhaps<T>, §9 Result<T, E>
**Blocked By:** none

## What

Two related gaps that require the same infrastructure:

1. **`enum_env` does not exist.** Task 0014 (match) plans to build it from `Decl::Enum`,
   but variant construction and path lookup (task 0020) also need it. The built-in types
   `Perhaps<T>` and `Result<T, E>` are never declared in user programs — they must be
   pre-populated separately.

2. **`Expr::StructLiteral` breaks for enum variant construction.** `Shape::Circle { radius: 5.0 }`
   and `Result::Ok { value: x }` parse as `Expr::StructLiteral { path: ["Shape", "Circle"], ... }`.
   The current handler calls `path.last()` and looks in `struct_env` — finding nothing named
   `"Circle"` or `"Ok"` and returning E0003. The fix: detect two-segment paths and route them
   through `enum_env` instead.

This task **unblocks** task 0014 (which currently plans to build `enum_env` itself but should
consume the one built here) and task 0020 (`Expr::Path` for unit variants).

## `InferContext` additions

Add to `typeinference/mod.rs`:

```rust
pub struct VariantInfo {
    pub name:   String,
    pub fields: Vec<(String, InferType)>,  // empty vec for unit variants
}

// In InferContext:
pub enum_env: HashMap<String, Vec<VariantInfo>>,

pub fn register_enum(&mut self, name: String, variants: Vec<VariantInfo>) {
    self.enum_env.insert(name, variants);
}

pub fn get_enum_variants(&self, name: &str) -> Option<&Vec<VariantInfo>> {
    self.enum_env.get(name)
}
```

Initialize `enum_env: HashMap::new()` in `InferContext::new()`.

## Pre-pass

Extend `hoist_struct_and_impl_decls` (or a new `hoist_enum_decls`) to walk `Decl::Enum`
and register each enum's variants:

```rust
Decl::Enum(ed) => {
    let variants = ed.variants.iter().map(|v| VariantInfo {
        name:   v.name.clone(),
        fields: v.fields.iter()
            .map(|f| (f.name.clone(), type_expr_to_infer(&f.type_ann)))
            .collect(),
    }).collect();
    ctx.register_enum(ed.name.clone(), variants);
}
```

After processing all declarations, pre-populate the built-in generic types. Use fresh type
variables for each registration so that each construction site gets independent vars:

```rust
fn register_builtin_enums(ctx: &mut InferContext) {
    // Perhaps<T>: variants Some { value: T } and Nope (unit)
    let t = ctx.fresh_var();
    ctx.register_enum("Perhaps".into(), vec![
        VariantInfo { name: "Some".into(), fields: vec![("value".into(), t)] },
        VariantInfo { name: "Nope".into(), fields: vec![] },
    ]);
    // Result<T, E>: variants Ok { value: T } and Err { error: E }
    let t = ctx.fresh_var();
    let e = ctx.fresh_var();
    ctx.register_enum("Result".into(), vec![
        VariantInfo { name: "Ok".into(),  fields: vec![("value".into(), t)] },
        VariantInfo { name: "Err".into(), fields: vec![("error".into(), e)] },
    ]);
}
```

Note: the fresh vars for built-in enums mean each `Result::Ok { value: x }` construction
gets its own fresh `T` and `E`, which is correct — the field constraint binds `T` to the
actual value type, and the enclosing annotation or usage pins `E`.

## `Expr::StructLiteral` fix in `infer_expr`

Distinguish single-segment paths (plain struct) from two-segment paths (enum variant):

```rust
Expr::StructLiteral { path, fields, span } => {
    if path.len() == 2 {
        let enum_name    = &path[0];
        let variant_name = &path[1];
        let variants = ctx.get_enum_variants(enum_name)
            .ok_or_else(|| YoloscriptError::type_error(
                ErrorCode::E0003,
                format!("unknown enum `{enum_name}`"),
                span,
            ))?
            .clone();
        let variant = variants.iter().find(|v| v.name == *variant_name)
            .ok_or_else(|| YoloscriptError::type_error(
                ErrorCode::E0003,
                format!("no variant `{variant_name}` on enum `{enum_name}`"),
                span,
            ))?;
        for (fname, expr) in fields {
            let decl_ty = variant.fields.iter()
                .find(|(n, _)| n == fname)
                .map(|(_, ty)| ty.clone())
                .ok_or_else(|| YoloscriptError::type_error(
                    ErrorCode::E0003,
                    format!("no field `{fname}` on `{enum_name}::{variant_name}`"),
                    span,
                ))?;
            let expr_ty = infer_expr(expr, ctx, fun_generalizations)?;
            ctx.add_constraint(expr_ty, decl_ty, span.clone());
        }
        Ok(InferType::Named(enum_name.clone(), vec![]))
    } else {
        // existing single-segment struct literal logic (unchanged)
        ...
    }
}
```

The return type is `Named("Shape", [])` — type parameters are not tracked in the Named
wrapper at this stage. The field constraints still ensure type correctness.

## Pass 2

Add `build_concrete_enum_env` parallel to `build_concrete_struct_env`:

```rust
fn build_concrete_enum_env(
    enum_env: &HashMap<String, Vec<VariantInfo>>,
    subst: &Substitution,
) -> Result<HashMap<String, Vec<ConcreteVariantInfo>>, YoloscriptError> {
    let dummy = Span::new(0, 0, "");
    enum_env.iter()
        .map(|(ename, variants)| {
            let cv = variants.iter().map(|v| {
                let fields = v.fields.iter()
                    .map(|(n, ty)| Ok((n.clone(), infer_type_to_type(&subst.apply(ty), &dummy)?)))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(ConcreteVariantInfo { name: v.name.clone(), fields })
            }).collect::<Result<Vec<_>, _>>()?;
            Ok((ename.clone(), cv))
        })
        .collect()
}
```

Add `enum_env: HashMap<String, Vec<ConcreteVariantInfo>>` to `ConstructCtx`. In
`construct_expr` for `Expr::StructLiteral`, use the same two-branch logic.

## Update task 0014

Task 0014 should be updated to say `Blocked By: 0019` and remove its own `enum_env`
construction — it should consume the env built here.

## Acceptance Criteria

- [ ] `VariantInfo` struct added to `typeinference/mod.rs`
- [ ] `enum_env` field and helpers added to `InferContext`
- [ ] Pre-pass populates `enum_env` from all `Decl::Enum` declarations
- [ ] `Perhaps` and `Result` are pre-populated with their variants and field types
- [ ] `Expr::StructLiteral` with two-segment path routes through `enum_env`
- [ ] Positive test: user-defined enum variant construction (`Shape::Circle { radius: 5.0 }`)
- [ ] Positive test: `Result::Ok { value: 42 }` typechecks
- [ ] Positive test: `Result::Err { error: "oops" }` typechecks
- [ ] Negative test: unknown variant name → E0003
- [ ] Negative test: field type mismatch in variant construction → E0001
- [ ] Pass 2 constructs `TypedExpr::StructLiteral` correctly for enum variant paths
- [ ] All prior tests still pass

## Notes

The return type `Named("Shape", [])` omits type parameters. For generic enums, the type
parameter is inferred indirectly through field constraints rather than explicitly tracked in
the Named wrapper. This is sufficient for the evaluator in v0.1.
