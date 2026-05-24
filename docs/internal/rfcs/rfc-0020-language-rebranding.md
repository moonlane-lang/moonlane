---
id: rfc-0020
title: "Language Keyword Rebranding"
date: '2026-05-23'
status: incorporated
---

## Summary

Rename a set of language keywords and builtin identifiers to align with the Moonlane wind theme. This is a **breaking change** targeting v0.2 — no edition gating. Three changes are accepted: `nope` → `None`, `trait` → `aspect`, and `mod` → `harness` (reserved for the future module system). One item is deferred: the `.yolo()` builtin rename, pending the stdlib migration of `Result` and `Perhaps`.

---

## Motivation

The language is named **Moonlane**. The current keyword set is a mix of Rust-influenced names (`trait`) and prototype-era playful names (`nope`) from an earlier development phase. A coherent naming strategy signals the language's identity and removes misleading familiarity: `trait` in Moonlane does not carry Rust's semantics (no orphan rules, no lifetime bounds, no `dyn` dispatch by default), so using the same word invites incorrect assumptions.

---

## Accepted Changes

### 1. `nope` → `None`

The `Perhaps` type's empty variant is renamed from `nope` (or `Perhaps::Nope`) to `None`.

```moonlane
// Before
let x: Perhaps<Int> = nope;

// After
let x: Perhaps<Int> = None;
```

**Rationale:** `None` is the near-universal spelling for an absent optional value (Python, Swift, Kotlin, ML family). `nope` is memorable but teaches the wrong habit and surprises every developer familiar with typed languages.

---

### 2. `trait` → `aspect`

Behaviour contracts are declared with `aspect` instead of `trait`.

```moonlane
// Before
trait Comparable {
    fun compare(other: Self) -> Int;
}

impl Comparable for Point { ... }

// After
aspect Comparable {
    fun compare(other: Self) -> Int;
}

impl Comparable for Point { ... }
```

**Rationale:** `aspect` is thematically grounded (an aspect of the wind, a facet of a surface facing the wind) and distinct enough from Rust's `trait` to signal semantic differences. It is also unambiguous — unlike `face`, `vane`, or `current`, it does not collide with common field names or control-flow vocabulary.

**Impact on RFC-0002:** All `trait` occurrences in the trait bound syntax proposal become `aspect`.

---

### 3. `harness` — reserved for the module system

`harness` is reserved as the module declaration keyword, to be activated when the module system (RFC-0009) is implemented.

```moonlane
// Future syntax (RFC-0009)
harness math {
    pub fun sqrt(x: Float) -> Float { ... }
}

use math::sqrt;
```

**Rationale:** A harness holds and channels multiple components — fitting for a module boundary. It is consistent with the language name and meaningfully different from `mod` (which carries Rust baggage about visibility and file layout). Reserving the keyword now avoids a parse-level breaking change when the module system lands.

---

## Deferred: `.yolo()` Method Rename

The `.yolo()` builtin (if it exists as a method) is deferred. `Result` and `Perhaps` are planned for migration into the standard library; the rename will be decided as part of that work, when the full method surface of both types is known.

Candidates noted for that future RFC:

| Candidate | Notes |
|---|---|
| `.yolo()` | Self-referential, memorable |
| `.blow()` | Direct wind action; terse |
| `.squall()` | Fits panic semantics — sudden, violent |
| `.breeze()` | Connotation: effortless — fits a "just give me the value" call |
| `.updraft()` | Evokes surfacing a value |

---

## Unchanged Keywords

All other keywords remain as-is: `fun`, `let`, `mut`, `struct`, `enum`, `type`, `match`, `for`, `while`, `return`, `break`, `continue`, `if`, `else`, `use`, `pub`, `impl`, `where`, `in`, `as`. The language is not undergoing a wholesale retheme — only the three items above are changed.

---

## Impact Summary

| Item | Change | When |
|---|---|---|
| `nope` / `Perhaps::Nope` | → `None` / `Perhaps::None` | v0.2 (breaking) |
| `trait` keyword | → `aspect` | v0.2 (breaking) |
| `harness` | reserved keyword | v0.2 (reserved, activated in RFC-0009) |
| `.yolo()` | deferred | stdlib migration phase |
| File extension `.mln` | adopted | v0.1 |

---

## Open Questions

1. **`Some` variant** — `Perhaps::Some` is unchanged. If `None` is accepted, `Some` stays for consistency with the ML/Kotlin/Swift tradition. No wind-themed rename proposed.

2. **`impl Aspect for Type` phrasing** — `impl` is unchanged. The full syntax reads `impl Comparable for Point`, which remains clear. No action needed.

---

## Decision

**Outcome:** *(pending)*
**Target:** v0.2

*(Decision rationale goes here when the RFC is evaluated.)*
