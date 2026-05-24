---
id: rfc-0018
title: "Match Arm Blocks"
date: '2026-05-22'
status: incorporated
---

## Summary

Allow match arm bodies to be either a single expression or a block (`{ stmts* expr? }`), consistent with every other construct in the language that has a body (`if`, `loop`, `fun`, closures).

---

## Motivation

The current grammar restricts match arm bodies to a single expression:

```
match_arm = { pattern ~ ("if" ~ expr)? ~ "=>" ~ expr }
```

This means any arm that needs more than one step requires hoisting logic outside the match or wrapping it in an immediately-called closure — both of which are unnatural. `if`, `loop`, and closures all accept blocks; match arms should too.

```moonlane
// currently impossible — has to be hoisted or crammed into one expression
match result {
    Result::Ok { value } => {
        let processed = transform(value);
        println(processed);
        processed
    },
    Result::Err { error } => {
        log(error);
        default_value
    },
}
```

---

## Proposal

### Grammar

```pest
match_arm = { pattern ~ ("if" ~ expr)? ~ "=>" ~ (block | expr) }
```

`block | expr` — the parser tries `block` (starts with `{`) first, then falls back to `expr`. Because `block` and `expr` are unambiguous at their first token (`{` vs anything else), there is no parsing conflict.

A bare `expr` arm is sugar for a block with no statements and the expression as the tail — they are semantically identical.

### AST

`MatchArm.body` changes from `Expr` to `Block`. Bare expression arms are wrapped by the parser:

```rust
// expr arm  =>  Block { stmts: vec![], tail: Some(Box::new(expr)), span }
```

### Typed AST

`TypedMatchArm.body` changes from `TypedExpr` to `TypedBlock`, consistent with `if` branches, loop bodies, and closure bodies.

### Typechecker

`construct_match` calls `construct_block` for each arm body instead of `construct_expr`. The arm's type is its block's tail type (`block.tail.as_ref().map(|e| e.ty()).unwrap_or(Type::Unit)`). The existing scope push/pop for pattern bindings is retained; `construct_block` adds a child scope inside it, so pattern bindings remain visible throughout the block.

### Evaluator

`eval_expr(&arm.body, env)` becomes `eval_block(&arm.body, env)`. Pattern bindings are still defined in the outer scope push before the block evaluates, so they are visible inside the block.

### Spec

The match expression section in `docs/public/spec/expressions.md` is updated to show block bodies in examples and to document that both forms are valid:

```moonlane
match value {
    pattern => expression,           // single-expression arm
    pattern => { stmts* expr? },    // block arm
}
```

---

## Alternatives Considered

**Add `block` to `primary_expr` (bare block expressions everywhere).** This would also fix match arms, since `block` would become a valid `expr`. Rejected in favour of the targeted fix: bare block expressions are not motivated by any other use case today, and adding them globally could interact unexpectedly with struct literal syntax (`{` as first token). This can be revisited independently.

---

## Open Questions

None — design is fully specified above.

---

## Timing Recommendation

Small, self-contained change. No design risk. Implement alongside other v0.2 grammar and parser work.

---

## References

- Language spec: `docs/public/spec/expressions.md`
- Grammar: `moonlane-interpreter/src/grammar.pest`

## Decision

**Outcome:** Accepted
**Target:** v0.2

Unambiguous fix — match arms are the only body-bearing construct in the language that does not accept a block. All other constructs (`if`, `loop`, `fun`, closures, `while`, `for`) use `block`. The design follows the existing pattern exactly.
