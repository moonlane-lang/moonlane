# Task 0002: `From` Trait and `as` Cast Upgrade

**Status:** open
**Epic:** epic-004-traits
**Component:** typechecker, evaluator
**Spec Link:** `../01-SPEC/LANGUAGE-SPEC.md` §3.5 Type casting (`as`), §10 Traits
**Blocked By:** Epic 004 trait infrastructure (trait definition checking, impl checking, trait-bound lookup)

## What

The spec states (§3.5):

> The `as` operator casts between numeric primitive types. It desugars to a call
> to the `From` trait and is infallible — the result is the target type directly.
> Because `as` desugars to `From`, user-defined types become castable by
> implementing `From<SourceType>` for the target type.

Epic 001 task 0017 implements a provisional check: both source and target are
constrained to the same fresh type variable, which enforces that they are in the
same numeric family. This catches `"hello" as Int` via the constraint solver but
does not use the `From` trait at all.

This task replaces that with proper trait dispatch: `x as T` resolves to
`T::from(x)`, and validity is determined by whether `T` implements `From<S>`.

## Design

### The `From<S>` trait

```yoloscript
trait From<S> {
    fun from(source: S) -> Self;
}
```

### Built-in implementations

Pre-register in the trait impl table before user code runs:

| Impl | Method signature |
|------|-----------------|
| `impl From<Float> for Int` | `fun from(source: Float) -> Int` |
| `impl From<Int> for Float` | `fun from(source: Int) -> Float` |
| `impl From<T> for T` (identity) | `fun from(source: T) -> T` (optional, for completeness) |

### Typechecking `as`

Replace the constraint-based check in `infer_expr`:

```rust
Expr::Cast { expr, target_type, span } => {
    let source_ty = infer_expr(expr, ctx, fun_generalizations)?;
    let source_ty = ctx.solve()?.apply(&source_ty);
    let target_ty = type_expr_to_infer(target_type);

    // Verify target implements From<source_ty> via the trait impl table.
    ctx.require_trait_impl(&source_ty, &target_ty, "From", span)?;
    // require_trait_impl emits E0001 if no impl exists.

    Ok(target_ty)
}
```

### Evaluator

Replace the hardcoded numeric coercion in the evaluator with a dispatch to
the registered `From` impl. The dispatch mechanism is whatever Epic 004
establishes for all trait method calls.

### User-defined casts

Once this task is done, a user can write:

```yoloscript
struct Celsius { degrees: Float }
struct Fahrenheit { degrees: Float }

impl From<Celsius> for Fahrenheit {
    fun from(source: Celsius) -> Fahrenheit {
        return Fahrenheit { degrees: source.degrees * 1.8 + 32.0 };
    }
}

let c = Celsius { degrees: 100.0 };
let f = c as Fahrenheit;
```

## Acceptance Criteria

- [ ] `From<S>` trait is defined and registered as a built-in trait
- [ ] `impl From<Float> for Int` and `impl From<Int> for Float` are pre-registered
- [ ] `as` typechecking uses `require_trait_impl` instead of the shared-var constraint
- [ ] `Int as Float` and `Float as Int` still pass (via built-in impls)
- [ ] `"hello" as Int` → E0001 via the trait resolver (no `From<String>` impl for `Int`)
- [ ] User-defined `impl From<S> for T` makes `x as T` valid
- [ ] Evaluator dispatches `as` through the `From` impl
- [ ] All Epic 001 cast tests still pass
- [ ] Positive test: user-defined cast via `impl From<S> for T`
- [ ] Negative test: cast with no impl → E0001

## Notes

The provisional check in Epic 001 task 0017 has a forward reference to this task.
Once this task is done, remove the `> Provisional:` note from task 0017 and mark
the relevant acceptance criterion as no longer provisional.
