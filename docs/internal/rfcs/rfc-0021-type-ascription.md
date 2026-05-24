---
id: rfc-0021
title: "Type Ascription Syntax"
date: '2026-05-23'
status: incorporated
---

## Summary

Introduce `:` as a type ascription operator in expression position, and preserve `as` for explicit runtime conversions. `:` reads as "this expression has type T" and guides type inference without any runtime cost. `as` means "convert this value to type T" and is a safe, explicit, runtime operation.

---

## Motivation

The current grammar has:

```
cast_expr = { unary_expr ~ ("as" ~ type_expr)* }
```

This was implemented with only numeric casts in mind (`Int as Float`). It fails on any other use:

```moonlane
let n = [] as String[];  // T0007: only Int as Float and identity casts are supported
```

The current implementation conflates two distinct concepts that warrant separate syntax:

- **Type ascription** — a compile-time hint to the inference engine. No runtime cost, no conversion. "This expression is of type T."
- **Explicit conversion** — a runtime operation that produces a value of a different type. "Convert this value to type T."

`as` is the right operator for explicit conversions. Rust's `as` is safe and explicit (not unsafe — no `unsafe` block required); Swift's `as` is safe for upcasts and checked downcasts. The common thread is that `as` signals a runtime operation the programmer is taking deliberate responsibility for. That is the correct home for `1 as Float`.

What Moonlane is missing is the *other* operator: a way to annotate an expression's type for inference purposes, without any conversion. That is type ascription, and `:` is the established form (Scala uses `expr: Type` in expression position).

The practical gap: without expression-level ascription, there is no way to write an empty array or empty `Perhaps` literal unless the binding already has a type annotation:

```moonlane
// This works — annotation is on the binding
let n: String[] = [];

// This should also work — annotation is on the expression
let n = [] : String[];

// Needed when passing an empty literal directly as an argument
foo([] : String[]);
```

The second and third forms are impossible without expression-level type ascription.

---

## Proposed Design

### Grammar change

```pest
// Before
cast_expr = { unary_expr ~ ("as" ~ type_expr)* }

// After
asc_expr = { unary_expr ~ (":" ~ type_expr)? }
```

`as` is removed from the grammar entirely and becomes an ordinary identifier (or is reserved for future use). `:` in expression position means "ascribe this type to the sub-expression."

**Disambiguation:** `:` already appears in:
- `let n: Int[] = []` — part of the `let_decl` / `mut_decl` grammar rule, before `=`. This is syntactically distinct: the parser is in a declaration context, not an expression context.
- `Point { x: 1, y: 2 }` — struct literal field syntax, inside `{}`. Also syntactically distinct.
- `match` arm patterns — `pattern => body`, no `:` involved.

In all three cases the parse context is unambiguous. A `:` in expression position (after a complete sub-expression, within `asc_expr`) cannot be confused with the declaration or struct-field `:`.

### Semantics

Type ascription is **not** a runtime operation. It is a compile-time annotation that:

1. Constrains the sub-expression's inferred type to the ascribed type.
2. Passes the ascribed type as `expected_ty` into the sub-expression's construction, enabling inference to flow inward (e.g. resolving empty array element types).

It is a type error if the inferred type of the sub-expression cannot be unified with the ascribed type.

```moonlane
let n = [] : String[];     // ok — element type resolved to String
let x = 1 : Int;           // ok — identity ascription
let y = 1 : String;        // type error — Int is not String
foo([] : String[]);         // ok — ascription in argument position
```

Ascription is **not** a conversion. `1 : Float` is a type error, not a conversion of the integer `1` to a float. Numeric conversions, if needed, are a separate language feature.

### Inference pass (`src/typechecker/inference.rs`)

In the `Expr::Cast` branch (rename to `Expr::Ascribe` or reuse the same AST node):

```rust
let inner_ty = infer_expr(inner, ctx, fun_generalizations)?;
let ascribed_ty = type_expr_to_infer(ann);
ctx.add_constraint(inner_ty.clone(), ascribed_ty, span.clone());
Ok(inner_ty)
```

### Construction pass (`src/typechecker/construction.rs`)

```rust
Expr::Ascribe(inner, ann, span) => {
    let ty = resolved_to_type(&type_expr_to_infer(ann), ctx.subst, span)?;
    construct_expr(inner, Some(&ty), ctx)
}
```

The ascribed type becomes `expected_ty` for the inner expression. This is the key change that makes `[] : String[]` work: `expected_ty = Some(Type::Array(String))` flows into the empty array branch.

---

## Preserved and extended: `as` for explicit conversions

`as` is kept as the explicit runtime conversion operator. It is **not** unsafe — in Rust, `as` requires no `unsafe` block; in Swift, `as` is safe for upcasts and checked downcasts. The word signals "I am deliberately converting this value," which is exactly the right mental model.

The two operators are now cleanly distinct:

| Operator | Kind | Runtime cost | Valid when |
|---|---|---|---|
| `: T` | type ascription | none | inferred type unifies with T |
| `as T` | explicit conversion | yes | a conversion from the value's type to T is defined |

```moonlane
let n = [] : String[];     // ascription — no conversion, inference hint only
let x = 1 as Float;       // conversion — produces a Float at runtime
let y = 3.14 as Int;      // conversion — truncates
let z = 1 : Float;        // type error — Int is not Float; use `as` to convert
```

The existing `as` implementation (currently limited to `Int as Float` and identity casts) is extended rather than removed. Full conversion coverage (including user-defined conversions via `From`) is tracked in #12.

---

## AST Impact

The `Expr::Cast` variant can be renamed to `Expr::Ascribe` or left as-is with updated semantics. The stored data is the same: `(inner: Expr, annotation: TypeExpr, span: Span)`.

No new grammar rule is strictly required — `cast_expr` can be renamed `asc_expr` in the grammar for clarity, but the AST variant and internal name can evolve separately.

---

## Open Questions

1. **Chained ascription — `x : A : B`?** The grammar allows `(":" ~ type_expr)?` (single) or could allow `*` (multiple). Multiple ascriptions are redundant but not harmful. Proposed: allow at most one (the `?` form) — a second `:` is a parse error, steering users toward `x : A` or rewriting.

2. **Ascription vs conversion** — ✅ Resolved: `1 : Float` is a type error (Int does not unify with Float). Use `1 as Float` for the conversion.

3. **`as` keyword** — ✅ Resolved: `as` is preserved as the explicit runtime conversion operator. Not removed.

---

## Decision

**Outcome:** *(pending)*
**Target:** v0.2

*(Decision rationale goes here when the RFC is evaluated.)*
