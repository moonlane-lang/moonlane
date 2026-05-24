---
id: rfc-0016
title: "Standard Library Foundation"
date: '2026-05-21'
status: draft
---

## Summary

Define the initial scope and organisation of the Moonlane standard library: which modules exist, what they contain, and how they are imported. Blocked on RFC-0009 (module system).

---

## Motivation

All v0.1 programs are single-file with no stdlib beyond the [built-in functions](../../public/spec/runtime.md#built-in-functions). A standard library is needed for practical programs: numeric utilities, string manipulation, I/O, and higher-level collections.

---

## Planned Modules

### `std::math`

| Function | Signature |
|---|---|
| `floor` | `(f: Float) -> Float` |
| `ceil` | `(f: Float) -> Float` |
| `abs` | `(n: Int) -> Int` / `(f: Float) -> Float` |
| `sqrt` | `(f: Float) -> Float` |
| `pow` | `(base: Float, exp: Float) -> Float` |
| `min`, `max` | `(a: T, b: T) -> T` |

### `std::string`

| Function | Signature |
|---|---|
| `split` | `(s: String, sep: String) -> String[]` |
| `trim` | `(s: String) -> String` |
| `contains` | `(s: String, sub: String) -> Bool` |
| `to_upper`, `to_lower` | `(s: String) -> String` |

### `std::io`

| Function | Signature |
|---|---|
| `read_line` | `() -> String` |
| `read_file` | `(path: String) -> Result<String, String>` |
| `write_file` | `(path: String, content: String) -> Result<(), String>` |

### `std::collections`

- `List<T>`: higher-level sequence type on top of `Array<T>`, with `push`, `pop`, `map`, `filter`, `fold`. Requires generics (v0.3).

---

## Built-in Migration

The v0.1 built-in functions (`print`, `println`, `int_to_string`, `float_to_string`, `bool_to_string`, `string_len`, `string_concat`, `array_push`, `array_len`, `clock`) are an explicit **temporary measure**. They exist only because v0.1 programs have no module system and no stdlib. When the stdlib ships, these functions move into appropriate stdlib modules and the global built-in form is removed:

| Built-in          | Stdlib destination            |
|-------------------|-------------------------------|
| `print`, `println`| `std::io`                     |
| `int_to_string`, `float_to_string`, `bool_to_string` | superseded by the `Display` trait (RFC-0012) |
| `string_len`, `string_concat` | `std::string`    |
| `array_push`, `array_len`     | `std::array` / `std::collections` |
| `clock`           | `std::time`                   |

This is not a backwards-compatibility question — built-ins are not a stable API surface. Programs written for v0.1 are expected to migrate when stdlib lands.

## Open Questions

- **Module path convention**: `std::math` or `std/math` or something else? Depends on RFC-0009.
- **Generic stdlib functions** (`min`, `max`, `List<T>`): require the trait system for bounds (`T: Comparable`). Does stdlib ship in phases alongside language versions?
- **Versioning**: does the stdlib follow the same language version as the spec, or does it version independently?

---

## Decision

**Outcome:** *(pending)*  
**Target:** *(blank until accepted)*

*(Decision rationale goes here when the RFC is evaluated.)*
