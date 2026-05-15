# Task 0012: `for` and `for-in` Statement Typechecking

**Status:** open
**Epic:** epic-001-typechecker
**Component:** typechecker
**Spec Link:** `../01-SPEC/LANGUAGE-SPEC.md` §12.3 C-style for, §12.4 For-in
**Blocked By:** none

## What

`Stmt::For` and `Stmt::ForIn` both fall through to `_ => internal error` in
`infer_stmt` and `construct_stmt`. Any program using a loop over an array or
range fails at the typechecker boundary.

## Typing Rules

### `Stmt::For`

```yoloscript
for (mut i = 0; i < 10; i += 1) { ... }
```

- **Init** (`ForInit::Mut(md)`): treat exactly like `Decl::Mut` — infer value,
  bind in a new scope wrapping the loop.
- **Condition** (`Option<Expr>`): if present, must unify with `Bool`.
- **Step** (`Option<Expr>`): infer but discard type (it's evaluated for
  side effects; typically an assignment, which produces `Unit`).
- **Body**: infer the block; discard its type (for-loops produce `Unit`).
- **Scope**: init introduces a new scope that encloses condition, step, and body.
  Pop after the loop.

Returns `InferType::unit()`.

### `Stmt::ForIn`

```yoloscript
for (let item in collection) { ... }
for (let i in 0..10) { ... }
```

- **Iterable**: infer iterable expression. Must unify with either:
  - `Array(T)` for array iteration
  - `Named("Range", [Int])` for range iteration (`0..10`, `0..=10`)
- **Binding**: the binding `item` / `i` gets type `T` (element type) for
  arrays, or `Int` for ranges.
- **Body**: infer block in a new scope containing the binding; discard result.
- The `binding` field in the AST is a plain `String` (not a `Param`), so
  it is always immutable. Bind it with `is_mutable: false`.

Returns `InferType::unit()`.

> **Provisional:** the hardcoded `Array(T) | Range<Int>` union check is a v0.1
> simplification. Epic 004 replaces it with a proper `Iterable<T>` trait lookup
> (see epic-004 task 0001). At that point user-defined iterables also become
> supported. No rework is needed here until then.

## Pass 2

`construct_stmt` needs matching arms:

- `Stmt::For`: construct init expr/decl, condition, step, and body. Bind init
  variable in scope like a `Mut` decl. The `TypedForInit` enum has `Mut` and
  `Expr` variants, matching `ForInit`.
- `Stmt::ForIn`: construct iterable, determine element type from the concrete
  iterable type, bind the loop variable, construct body.

Both return `TypedStmt::For` / `TypedStmt::ForIn`.

## Acceptance Criteria

- [ ] `Stmt::For`: init, condition, step, body all typechecked; condition constrained to `Bool`
- [ ] `Stmt::For`: init variable is in scope for condition, step, and body
- [ ] `Stmt::ForIn`: iterable constrained to `Array(T)` or `Range<Int>` (provisional — see Epic 004 task 0001); binding gets type `T` / `Int`
- [ ] Both produce `Unit`
- [ ] Pass 2 constructs `TypedStmt::For` and `TypedStmt::ForIn` correctly
- [ ] Positive test: `for` over an index counter, `for-in` over an array and a range
- [ ] Negative test: non-Bool condition in `for` → E0001
- [ ] Negative test: `for-in` over a non-array, non-range value → E0001
- [ ] All prior tests still pass
