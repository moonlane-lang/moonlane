---
id: rfc-0022
title: "Braceless if body syntax"
date: '2026-05-23'
status: incorporated
target: v0.3
---

## Summary

Allow `if` expressions to accept a single expression as the body without requiring braces, e.g. `if (condition) expr;`.

---

## Motivation

Currently, every `if` body must be a block (`{ ... }`), even for trivial single-expression branches. This is unnecessarily verbose for simple cases like `if (debug) print_state();`. Many Rust-inspired languages (C, Swift, Kotlin) permit braceless single-expression bodies as a convenience form.

---

## Proposal

Extend the `if` grammar to accept either a `block` or a single `expr` as the body:

```
if_expr = { "if" ~ "(" ~ expr ~ ")" ~ (block | expr) ~ ("else" ~ (if_expr | block | expr))? }
```

A bare expression body is sugar for a block with that expression as its tail:

```
if (condition) expr;
// equivalent to
if (condition) { expr; }
```

**Expression context:** A braceless `if` without an `else` branch produces `Unit` and may only appear in statement position. A braceless `if`–`else` may appear in expression position if both branches have the same type, identical to braced `if`–`else`.

**Parser normalization:** The parser wraps the bare expression in a synthetic `Block` (same technique used today for `else if`), so no changes are required downstream in the type checker or evaluator.

---

## Alternatives Considered

**Reject braceless bodies entirely.** Braces are explicit and eliminate ambiguity. This is Moonlane's current behavior. Rejected because it is overly strict for trivial single-expression branches.

**Allow braceless bodies only in statement position.** Prevents use as an expression even with an `else` branch. Overly restrictive — `let x = if (flag) a else b;` is unambiguous and useful.

**Allow braceless bodies without a semicolon.** `if (condition) expr` with no terminator is syntactically ambiguous when followed by another expression on the same line. Requiring a `;` at the statement level avoids this ambiguity.

---

## Decisions

1. **Braceless `if`–`else` in expression position: allowed.** When both branches have matching types, a braceless `if`–`else` may appear in expression position, identical to the braced form.
2. **Nested braceless bodies: allowed only when the inner `if` has no `else`.** `if (a) if (b) expr;` is valid. `if (a) if (b) x; else y;` is a parse error — the outer body must use braces whenever the inner `if` has an `else` branch. This eliminates the dangling-else ambiguity entirely.
3. **Mixing braced and braceless arms: not allowed.** Both the `then` and `else` arms must use the same style — either both braced (`block`) or both braceless (`expr`). A parse error is emitted for mismatched styles.

---

## Timing Recommendation

Target v0.3. This is a pure syntax extension with no type system or evaluator impact. The three-file change (grammar, parser, spec) is self-contained and low risk.

---

## References

- Language spec: `docs/public/spec.md`
- `docs/public/spec/expressions.md` — `if` expression section
- `moonlane-interpreter/src/grammar.pest` line 151 — current `if_expr` rule
- `moonlane-interpreter/src/parser/mod.rs` line 513 — `parse_if_expr`
