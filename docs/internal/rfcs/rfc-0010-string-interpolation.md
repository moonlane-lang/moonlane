---
id: rfc-0010
title: "String Interpolation"
date: '2026-05-21'
status: draft
---

## Summary

Define syntax for embedding expressions inside string literals, eliminating the need for explicit `int_to_string` / `float_to_string` calls and `+` concatenation for formatted output.

---

## Motivation

Current string formatting requires:

```moonlane
println("x = " + int_to_string(x) + ", y = " + float_to_string(y));
```

This is verbose for a common operation. Most modern languages provide interpolation syntax.

---

## Open Questions

- **Syntax options**:
  - Backtick template literals: `` `hello ${name}` ``
  - Brace-in-double-quote: `"hello {name}"`
  - Sigil prefix: `f"hello {name}"` (Python/Rust style)
- **What's interpolatable**: any expression? Only types implementing a `Display` / `ToString` trait? The latter requires the trait system (v0.2) as a prerequisite.
- **Nested quotes**: how are quotes inside an interpolated expression handled?
- **Escape**: how is the interpolation delimiter escaped when needed literally (e.g. a literal `{` in the string)?
- **Multi-line strings**: does interpolation interact with multi-line string syntax (if that's ever added)?

---

## Decision

**Outcome:** *(pending)*  
**Target:** *(blank until accepted)*

*(Decision rationale goes here when the RFC is evaluated.)*
