# Task 0021: Built-in Function Registration

**Status:** open
**Epic:** epic-001-typechecker
**Component:** typechecker
**Spec Link:** `../01-SPEC/LANGUAGE-SPEC.md` §15 Built-in functions
**Blocked By:** none

## What

The §15 built-in functions (`println`, `print`, `int_to_string`, etc.) are never
registered in `InferContext`. Any program that calls one gets E0003 "undefined name
`println`" instead of a type error on the arguments. They must be pre-registered
before inference runs since they are available globally without a `use` declaration.

## Implementation

Add `register_builtins(ctx: &mut InferContext)` and call it at the top of `check()`,
before `hoist_fun_decls`:

```rust
fn register_builtins(ctx: &mut InferContext) {
    let str_ty   = InferType::str();
    let int_ty   = InferType::int();
    let float_ty = InferType::float();
    let bool_ty  = InferType::bool();
    let unit_ty  = InferType::unit();

    let mono = |params, ret| InferType::Fun(params, Box::new(ret));

    ctx.bind_mono("print",           mono(vec![str_ty.clone()], unit_ty.clone()), false);
    ctx.bind_mono("println",         mono(vec![str_ty.clone()], unit_ty.clone()), false);
    ctx.bind_mono("int_to_string",   mono(vec![int_ty.clone()], str_ty.clone()),  false);
    ctx.bind_mono("float_to_string", mono(vec![float_ty],       str_ty.clone()),  false);
    ctx.bind_mono("bool_to_string",  mono(vec![bool_ty],        str_ty.clone()),  false);
    ctx.bind_mono("string_len",      mono(vec![str_ty.clone()], int_ty.clone()),  false);
    ctx.bind_mono("string_concat",   mono(vec![str_ty.clone(), str_ty], str_ty.clone()), false);
    ctx.bind_mono("clock",           mono(vec![], int_ty.clone()), false);

    // Polymorphic built-ins: register as type schemes so they work at any element type.
    register_poly_builtin(ctx, "array_push", |t| {
        TypeScheme {
            quantified_vars: HashSet::from([t]),
            ty: InferType::Fun(
                vec![InferType::Array(Box::new(InferType::Var(t))), InferType::Var(t)],
                Box::new(InferType::unit()),
            ),
        }
    });
    register_poly_builtin(ctx, "array_len", |t| {
        TypeScheme {
            quantified_vars: HashSet::from([t]),
            ty: InferType::Fun(
                vec![InferType::Array(Box::new(InferType::Var(t)))],
                Box::new(InferType::int()),
            ),
        }
    });
}

fn register_poly_builtin<F>(ctx: &mut InferContext, name: &str, make_scheme: F)
where F: Fn(TypeVar) -> TypeScheme
{
    let v = ctx.var_gen.fresh();  // need to expose var_gen or add a helper
    ctx.bind_poly(name, make_scheme(v));
}
```

### Exposing `var_gen` for scheme construction

`TypeVarGenerator` is private inside `InferContext`. Two options:

**A)** Add a `fresh_type_var(&mut self) -> TypeVar` method to `InferContext` that returns
the raw `TypeVar` (not wrapped in `InferType::Var`) — needed to build `TypeScheme` which
takes `TypeVar` directly.

**B)** Add a convenience method `bind_poly_fun(&mut self, name, quantified_var_count, param_fn)`
that handles fresh var generation internally.

Option A is simpler.

### Pass 2

`ConstructCtx` also needs built-in function types in its scope. Add a parallel
`register_builtins_construct(ctx: &mut ConstructCtx)` that inserts concrete `Type::Fun`
values for the monomorphic built-ins, and stores the polymorphic schemes for `array_push`
and `array_len` so `construct_call` can instantiate them.

The simplest approach for Pass 2: add a `builtin_schemes: HashMap<String, TypeScheme>` to
`ConstructCtx` (separate from the user-function `scheme_env`) and check it in
`construct_call` alongside `scheme_env`.

## Acceptance Criteria

- [ ] All §15 built-ins are registered in `InferContext` before inference
- [ ] `println("hello")` typechecks without error
- [ ] `int_to_string(42)` typechecks and its result is `String`
- [ ] `array_push(arr, x)` typechecks for any element type `T`
- [ ] `array_len(arr)` typechecks for any array type and returns `Int`
- [ ] Wrong argument type to a built-in → E0001, not E0003
- [ ] `array_push` and `array_len` work at multiple distinct element types in the same program
- [ ] Pass 2 resolves built-in types in `ConstructCtx`
- [ ] Positive test: program calling `println`, `int_to_string`, `array_len`
- [ ] Positive test: `array_push` and `array_len` used with `Int[]` and `String[]` in the same program
- [ ] Negative test: wrong argument count to `println` → E0004
- [ ] All prior tests still pass
