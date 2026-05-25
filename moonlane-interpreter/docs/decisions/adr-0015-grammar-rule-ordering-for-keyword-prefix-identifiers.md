---
id: decision-15
title: "Grammar Rule Ordering to Allow Keyword-Prefix Identifiers"
date: '2026-05-25'
---

## Context

Identifiers like `break_sum`, `return_value`, `let_x` begin with a keyword prefix. In a PEG grammar, if `break_stmt` is tried before `expr_stmt` in the `stmt` rule, the parser greedily matches `"break"` as the keyword terminal — even though `"break_sum"` is a valid identifier token — and the parse fails.

This is a consequence of PEG's ordered choice (`/`): the first alternative that partially matches is committed to. The keyword rule matches the `break` prefix and expects either end-of-statement or a value expression; finding `_sum` instead causes a parse error.

## Options Considered

### Option A: Atomic keyword tokens with negative lookahead

Add atomic rules like `kw_break = @{ "break" ~ !(ASCII_ALPHANUMERIC | "_") }` and use them in `break_stmt` instead of the bare `"break"` literal.

**Why it fails in practice:** pest inserts the `WHITESPACE` rule before every token in a non-atomic parent rule. Because `WHITESPACE` can match zero bytes, the `!(ASCII_ALPHANUMERIC | "_")` lookahead runs _after_ optional whitespace has been consumed, not immediately after the keyword literal. This causes `let x = 5` to fail because after `"let"` the lookahead sees `x` (which is alphanumeric) in the next token, even though there's whitespace in between.

Making the _parent_ rule atomic (`@{ ... }`) would fix the whitespace issue but would require rewriting the entire `stmt` rule as atomic, losing all pest whitespace handling for the rule's content.

### Option B: Reorder alternatives in `stmt` (chosen)

Move `expr_stmt` before `break_stmt` and `return_stmt` in the `stmt` rule:

```pest
stmt = { while_stmt | for_stmt | for_in_stmt | continue_stmt
       | expr_stmt   // ← before break/return
       | return_stmt | break_stmt }
```

When the parser tries `expr_stmt` for `break_sum + 1`, it succeeds: `break_sum` is a valid identifier, and the expression parses correctly. `break_stmt` is never tried.

When the parser tries `expr_stmt` for `break;` (a bare break), `expr_stmt` tries to parse `break` as an expression. `break` is excluded from `ident` by the `!keyword` guard, so `expr_stmt` fails, and the parser falls through to `break_stmt`, which succeeds.

The same reordering is applied to `decl` (moving `stmt` before `let_decl`/`mut_decl` to fix `let_x`/`mut_x`) and to `for_init`.

**Invariant:** This ordering is correct because `break` and `return` as bare keywords cannot start a valid expression (they are excluded from `ident` by the `!keyword` guard in the `ident` rule). The ordering only affects keyword-prefix identifiers, never bare keywords.

## Decision

**Option B — reorder alternatives.** A comment block in `grammar.pest` above the relevant rules documents the invariant.

## Consequences

- Identifiers starting with any keyword prefix are now valid everywhere an identifier is expected.
- The `!keyword` guard on `ident` remains the ultimate gatekeeper: bare `break`/`return`/`let`/`mut` as standalone tokens cannot become identifiers regardless of rule order.
- Adding a new keyword in the future: if it needs to be a statement-initiator, it must be placed _after_ `expr_stmt` in the `stmt` rule.

## References

- #11–#13 — Sprint 6 integration tests revealed `break_sum` failing
- `src/grammar.pest` — see ordering comment block in `stmt`, `decl`, `for_init`
