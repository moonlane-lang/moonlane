# Task 0014: `match` Expression Typechecking

**Status:** open
**Epic:** epic-001-typechecker
**Component:** typechecker
**Spec Link:** `../01-SPEC/LANGUAGE-SPEC.md` §11 Pattern Matching
**Blocked By:** 0019 (enum_env is built there; 0014 consumes it rather than building it itself)

## What

`Expr::Match` falls through to `_ => internal error` in both `infer_expr` and
`construct_expr`. `match` is central to `Perhaps<T>`, `Result<T, E>`, and enum
usage — it blocks most real programs.

The AST nodes involved:

```rust
MatchExpr { scrutinee: Box<Expr>, arms: Vec<MatchArm>, span }
MatchArm  { pattern: Pattern, guard: Option<Expr>, body: Expr, span }

enum Pattern {
    Wildcard(Span),
    Nope(Span),
    Literal(Literal, Span),
    Binding(String, Span),
    EnumVariant { path: Vec<String>, fields: Vec<String>, span },
    Tuple(Vec<Pattern>, Span),
}
```

## Typing Rules

### Scrutinee

Infer the scrutinee's type: `scrutinee_ty = infer_expr(scrutinee)`.

### Pattern typechecking

Each pattern must be compatible with `scrutinee_ty`. A new function
`infer_pattern` returns the type the pattern matches against and binds
any variables it introduces into the current scope:

| Pattern | Action |
|---|---|
| `Wildcard` | Accept any type; bind nothing. |
| `Literal(lit)` | Constrain `scrutinee_ty == type_of(lit)`. |
| `Binding(name)` | Bind `name: scrutinee_ty` (immutable) in the arm scope. |
| `Nope` | Constrain `scrutinee_ty == Named("Perhaps", [fresh_var])`. |
| `Tuple(pats)` | Constrain `scrutinee_ty == Tuple([t0, t1, ...])` with fresh vars; recurse into sub-patterns against each `ti`. |
| `EnumVariant { path, fields }` | Look up the variant in `enum_env` (see below); constrain `scrutinee_ty` to the enum type; bind each field name to its declared type. |

### Enum environment

`enum_env` is built and pre-populated (including `Perhaps` and `Result`) by task 0019.
This task consumes it — `EnumVariant` patterns look up fields from `ctx.get_enum_variants()`.

### Arm bodies

All arm bodies must unify to a single type (the match expression's type):

```rust
let result_var = ctx.fresh_var();
for arm in arms {
    ctx.push_scope();
    infer_pattern(&arm.pattern, scrutinee_ty, ctx, ...)?;
    if let Some(guard) = &arm.guard {
        let g = infer_expr(guard, ctx, ...)?;
        ctx.add_constraint(g, InferType::bool(), guard_span);
    }
    let arm_ty = infer_expr(&arm.body, ctx, ...)?;
    ctx.add_constraint(arm_ty, result_var.clone(), arm.span.clone());
    ctx.pop_scope();
}
Ok(result_var)
```

### Exhaustiveness

**Defer.** Exhaustiveness checking requires tracking which patterns have been
covered, which is significant additional complexity. For this task, a non-
exhaustive match is a runtime error, not a compile-time error. Add a note in
the task doc that exhaustiveness checking is a future task.

## Pass 2

`construct_expr` for `Expr::Match`: construct scrutinee, then for each arm
construct the pattern bindings into scope, construct the guard and body, pop scope.
Returns `TypedMatchExpr` (already defined in `typed_ast`).

## Scope

**In scope:**
- `Wildcard`, `Literal`, `Binding`, `Nope`, `Tuple` patterns
- `EnumVariant` patterns for unit and struct-like variants (requires `enum_env`)
- Guard expressions (`if guard`)

**Deferred:**
- Exhaustiveness checking
- Nested enum variant patterns
- `EnumVariant` construction via `Expr::Path` (unit variants like `Direction::North`) — that is a separate expression-level concern

## Acceptance Criteria

- [ ] `infer_pattern` handles all six `Pattern` variants
- [ ] All arm bodies are constrained to unify; the match type is that common type
- [ ] Guard expressions are constrained to `Bool`
- [ ] Variables bound in patterns are in scope for the arm guard and body only
- [ ] `EnumVariant` patterns look up fields from `enum_env`
- [ ] Pass 2 constructs `TypedMatchExpr` correctly
- [ ] Positive test: `match` on `Int` literal patterns and `_`
- [ ] Positive test: `match` on `Perhaps<T>` with `Binding` and `Nope` arms
- [ ] Positive test: `match` on an enum with `EnumVariant` patterns
- [ ] Positive test: `match` with a guard expression
- [ ] Negative test: arm body type mismatch → E0001
- [ ] Negative test: non-Bool guard → E0001
- [ ] All prior tests still pass
