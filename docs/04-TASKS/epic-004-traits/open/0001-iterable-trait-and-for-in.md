# Task 0001: `Iterable<T>` Trait and `for-in` Upgrade

**Status:** open
**Epic:** epic-004-traits
**Component:** typechecker, evaluator
**Spec Link:** `../01-SPEC/LANGUAGE-SPEC.md` §12.4 For-in, §10 Traits
**Blocked By:** Epic 004 trait infrastructure (trait definition checking, impl checking, trait-bound lookup)

## What

`for-in` in Epic 001 (task 0012) uses a hardcoded union check:
the iterable must be `Array(T)` or `Range<Int>`. This works for v0.1 but prevents
user-defined types from being iterable.

This task replaces that provisional check with a proper `Iterable<T>` trait lookup,
making `for-in` open to any type that implements `Iterable<T>`.

## Design

### The `Iterable<T>` trait

```yoloscript
trait Iterable<T> {
    fun next(mut self) -> Perhaps<T>;
}
```

`for (let item in expr)` desugars to: repeatedly call `expr.next()` until it
returns `nope`; bind each `Some { value }` to `item`.

### Built-in implementations

Pre-register in the trait impl table (the same mechanism used for all
`impl Trait for Type` blocks, but populated before user code runs):

| Type | Implements | Element type |
|------|-----------|--------------|
| `T[]` | `Iterable<T>` | `T` |
| `Range` (`0..n`, `0..=n`) | `Iterable<Int>` | `Int` |

### Typechecking `for-in`

Replace the hardcoded union check in `infer_stmt`:

```rust
Stmt::ForIn(fi) => {
    let iter_ty = infer_expr(&fi.iterable, ctx, fun_generalizations)?;
    let iter_ty = ctx.solve()?.apply(&iter_ty);
    // Look up Iterable<T> impl for iter_ty.
    let elem_ty = ctx.lookup_trait_impl(&iter_ty, "Iterable")
        .and_then(|args| args.into_iter().next())  // first type arg = T
        .ok_or_else(|| YoloscriptError::type_error(
            ErrorCode::E0001,
            format!("type `{iter_ty}` does not implement `Iterable<T>`"),
            &fi.span,
        ))?;
    ctx.push_scope();
    ctx.bind_mono(&fi.binding, elem_ty, false);
    infer_block(&fi.body, ctx, fun_generalizations)?;
    ctx.pop_scope();
    Ok(InferType::unit())
}
```

### Evaluator

The evaluator also has a provisional union check for `for-in` (matching the
typechecker). Replace it with vtable / static dispatch through the `Iterable`
impl, consistent with however Epic 004 implements method dispatch.

## Acceptance Criteria

- [ ] `Iterable<T>` trait is defined and registered as a built-in trait
- [ ] `Array<T>` and `Range` are pre-registered as implementing `Iterable<T>`
- [ ] `for-in` typechecking uses `lookup_trait_impl` instead of the hardcoded union check
- [ ] User-defined type implementing `Iterable<T>` can be used in `for-in`
- [ ] Non-iterable type in `for-in` → E0001 "does not implement Iterable<T>"
- [ ] Evaluator dispatches `for-in` through the trait impl
- [ ] Positive test: `for-in` over a user-defined iterable type
- [ ] All Epic 001 `for-in` tests still pass (array and range iteration unchanged)

## Notes

The provisional hardcoded check in Epic 001 task 0012 has a forward reference to
this task. Once this task is done, remove the `> Provisional:` note from task 0012
and mark the acceptance criterion as no longer provisional.
