# ADR-0018: `None` is not in the keyword list

**Status:** Accepted  
**Sprint:** 8 (`sprint/8`)  
**Issue:** #151

---

## Context

`Perhaps::Nope` was renamed to `Perhaps::None` and the standalone `nope` literal became `None`. This creates a grammar ambiguity: `None` must work as a standalone literal (bare `None` in an expression position) AND as an identifier component in a path expression (`Perhaps::None`).

---

## Decision

`None` is **not** added to the keyword list in `grammar.pest`.

The `none_lit` rule (`"None" ~ !(ASCII_ALPHANUMERIC | "_")`) is placed **before** `path_expr` in both `primary_expr` and `pattern`. This ordering means:

- **Standalone `None`** is matched by `none_lit` first → parsed as `Literal::None` / `Pattern::None`.
- **`Perhaps::None`** is parsed as a `path_expr` because `None` is a valid `ident` (not a keyword), and the `::` separator makes it unambiguous.

---

## Invariant

**`none_lit` must appear before `path_expr` in both `primary_expr` and `pattern`.**

If `None` were added to the keyword list, `ident` would reject it and `Perhaps::None` would fail to parse as a path (since path segments are identifiers). If `none_lit` were moved after `path_expr`, standalone `None` might be caught as a single-segment path expression instead of a literal.

User-defined enum variants named `None` (e.g. `enum Maybe<T> { Some { value: T }, None {} }`) continue to work because `None` is a valid `ident`, and variant construction uses `path_expr` (`Maybe::None`), not `none_lit`.

---

## Alternatives considered

**Add `None` to the keyword list and special-case paths:** Rejected — would require the parser to reconstruct path expressions from keyword tokens, complicating the grammar significantly.

**Use a different keyword (e.g. `none`):** Rejected — RFC-0020 established `None` as the canonical spelling for alignment with Rust conventions.
