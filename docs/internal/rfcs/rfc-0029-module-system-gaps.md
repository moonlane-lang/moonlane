---
id: rfc-0029
title: "Module System — Gaps and Clarifications"
date: '2026-05-27'
status: draft
---

## Summary

RFC-0009 accepted the core module system design but left ten questions unresolved — some of which block a correct implementation. This RFC addresses them in order of severity: critical blockers first, then usability gaps, then minor ordering and deferral questions.

**Prerequisite:** RFC-0009 accepted.  
**Target:** v0.5.0 (same milestone — must be resolved before implementation begins).

---

## Open Questions

### OQ-1 — Module visibility (`pub mod`)

RFC-0009's `pub use` example references the concept of a module being private ("lexer itself remains private — not pub mod") without defining `pub mod`. The grammar only has `mod identifier;` with no visibility annotation.

Without this, it is unclear whether a `mod` declaration makes the submodule part of the declaring module's public API or an internal detail. Both cases arise in practice:

```moonlane
// parser/mod.mln

mod ast;       // internal — callers should not import crate::parser::ast directly
pub mod lexer; // public — crate::parser::lexer is part of the API
```

**Options:**

- **Option A — `pub mod` / `mod` distinction (Rust-style).** A bare `mod name;` declares a private submodule — it exists and is accessible within the declaring module, but `crate::parser::ast::*` is not a valid path for external callers. `pub mod name;` makes the submodule publicly reachable. `pub use` is still needed to re-export individual names from a private submodule.
- **Option B — All declared modules are implicitly public.** A `mod name;` declaration always makes `crate::…::name` reachable from outside. Module privacy is controlled entirely by `pub` on the individual declarations inside the module, not on the module itself. Simpler surface, but no way to hide an entire internal submodule without qualifying every item.
- **Option C — All declared modules are implicitly private; `pub use` is the only export path.** You can never import `crate::parser::ast::Ast` directly; you must go through re-exports declared in a `pub use` chain. Maximum encapsulation, but verbose for straightforward hierarchies.

---

### OQ-2 — `crate` root definition

RFC-0009 says `crate` refers to "the file containing the entry point" but does not define what that file is or how it is determined.

Two cases need a rule:

1. **Binary programs** (have `main()`): which file is the root? The file passed directly to the compiler? Always `main.mln`? A project manifest?
2. **Library modules** (no `main()`): what file is `crate::` rooted at? How does a caller of the library address its root?

**Options:**

- **Option A — `crate` root is always the file passed to the compiler CLI.** `moonlane run src/main.mln` makes `src/main.mln` the crate root. Libraries are compiled with `moonlane build src/lib.mln`. Simple, explicit, no manifest needed.
- **Option B — `crate` root is always a fixed filename.** Binary: `main.mln`. Library: `lib.mln`. The compiler looks for these names in the source root. Predictable, convention-based.
- **Option C — Project manifest (e.g. `moonlane.toml`) declares the entry point.** The compiler reads the manifest to find the root file. More infrastructure to define now but necessary for multi-target projects (binary + library in one project).

---

### OQ-3 — Name conflicts from multiple imports

When two `use` statements bring the same identifier into scope, the behaviour is undefined in RFC-0009.

```moonlane
use crate::parser::Token;
use crate::lexer::Token;   // conflict — what happens?
```

**Options:**

- **Option A — Compile error.** Any two `use` statements that would bind the same name in the current scope are rejected, regardless of whether the names refer to the same item or different items.
- **Option B — Compile error only when the name is actually used.** The conflict is reported at the use site, not at the `use` statement. Allows importing conflicting names as long as only one is referenced.
- **Option C — Last declaration wins.** The later `use` shadows the earlier one silently. Consistent with how local `let` bindings shadow, but surprising at the import level.

---

### OQ-4 — Ambiguous `mod` resolution

RFC-0009 says `mod name;` resolves to `name.mln` or `name/mod.mln`. When both files exist simultaneously, the behaviour is undefined.

**Decision:** This should be a **compile error** with a clear message. No option is worth considering — ambiguous module resolution is always a programmer mistake, and silently picking one would mask it.

The error message should name both candidate files:
```
error: ambiguous module `parser`
  both `parser.mln` and `parser/mod.mln` exist
  remove one to resolve the ambiguity
```

---

### OQ-5 — Import aliasing (`use … as`)

RFC-0009 provides no way to rename an import at its use site. This makes name conflicts unresolvable in the common case where two needed modules export the same name:

```moonlane
use crate::v1::Parser;
use crate::v2::Parser;   // conflict — no way to use both
```

Without aliasing, the only workaround is to not import one name and use its full path inline — but RFC-0009 also does not define whether qualified paths are valid in expression and type position without a `use` declaration.

**Two sub-questions:**

**OQ-5a — `use … as` aliasing.** Should `use path::to::Name as Alias` be valid in v0.5.0?

- **Option A — Yes, ship aliasing in v0.5.0.** Necessary for any program that uses two modules exporting the same name. Without it, name conflicts are entirely unresolvable.
- **Option B — Defer; require full paths for conflict resolution.** Only viable if OQ-5b is resolved in favour of allowing inline qualified paths.

**OQ-5b — Inline qualified paths without `use`.** Should `crate::parser::Ast` be valid in type and expression position without a corresponding `use` declaration?

- **Option A — Yes.** Any fully-qualified path is valid anywhere a name is expected. `use` is syntactic sugar for bringing a name into the local scope, not the only way to access an item.
- **Option B — No.** Items are only accessible by their short name after a `use` declaration. Full paths in expression position are not valid syntax.

These two questions interact: if both are "no" and "no", name conflicts are entirely unresolvable. At least one must be "yes."

---

### OQ-6 — Struct field visibility

RFC-0009 defines `pub` for top-level declarations but says nothing about struct fields. Two interpretations are possible:

```moonlane
pub struct Token {
    kind: TokenKind,   // accessible to importers? or private?
    span: Span,
}
```

**Options:**

- **Option A — Fields are public if the struct is public.** A `pub struct` exposes all its fields. Field-level privacy is not supported in v0.5.0. Simple, consistent with Moonlane's current model where struct literals are constructed by name.
- **Option B — Fields are private by default; `pub` per field.** Each field needs `pub` to be accessible from outside the module. Enables strong encapsulation of internal representation.
- **Option C — Fields follow the struct's visibility.** Fields inherit `pub` from the struct declaration unless individually overridden with a private annotation. Inverse of Option B.

Note: field-level visibility also interacts with struct literal construction. If a field is private, external code cannot construct the struct with a literal — it must use a constructor function. This is a significant ergonomic consequence of Option B.

---

### OQ-7 — `use module` vs. `use module::item` semantics

RFC-0009 shows `use std::math;` without defining what `math` means after the import. Two distinct semantics are possible:

```moonlane
use std::math;

let x = math::sin(1.0);   // (A) math is a module handle in scope — path prefix
let x = sin(1.0);         // (B) math is not usable — must use std::math::sin
```

**Options:**

- **Option A — `use path::module` brings the module into scope as a path handle.** `use std::math;` makes `math` a usable qualifier: `math::sin(x)`, `math::PI`, etc. Consistent with Go's import semantics.
- **Option B — `use` only binds the final name.** `use std::math;` brings the name `math` into scope as a type/value alias but not as a path prefix. To call `sin`, you need `use std::math::sin` or `use std::math::*`.

This question also determines whether `use std::collections::{Map, Set}` is the idiomatic pattern (Option B) or `use std::collections;` followed by `collections::Map` in code (Option A). The two styles are not mutually exclusive but the RFC should pick a primary idiom.

---

### OQ-8 — `std::core`: what it contains, how it is imported, and shadowing

RFC-0009 states that `Perhaps`, `Result`, `Bool`, `Int`, `Float`, and `String` "remain globally available in all programs regardless of module structure." Issue #150 proposes moving `Perhaps` and `Result` to a language core module. These two goals conflict: if the types are module-defined, they are no longer compiler built-ins in the traditional sense.

**What actually gets special treatment in the interpreter.**

A survey of the evaluator and typechecker reveals the full set of types and aspects that are hardcoded at the compiler level today:

| Name | Kind | Special treatment |
|---|---|---|
| `Perhaps<T>` | enum | Dedicated `Value::Perhaps` variant; `nope` literal desugars to `Perhaps::None`; pattern exhaustiveness hardcoded |
| `Result<T, E>` | enum | Dedicated `Value::Result` variant; `?` operator desugars to `Result::Err` propagation; pattern exhaustiveness hardcoded |
| `Range` | struct | `..` operator produces `Range`; `for-in` loop has hardcoded `Range` iteration |
| `RangeInclusive` | struct | `..=` operator produces `RangeInclusive`; `for-in` loop has hardcoded iteration |
| `Display` | aspect | `print` / `println` builtins dispatch through `Display::to_string` |
| `Iterable` | aspect | `for-in` loop dispatches through `Iterable::next` for non-`Range` types |
| `From` | aspect | `?` coercion and numeric conversion dispatch through `From::from` |
| `Int` | primitive | Dedicated `Value::Int` variant; arithmetic operators hardcoded |
| `Float` | primitive | Dedicated `Value::Float` variant; arithmetic operators hardcoded |
| `Bool` | primitive | Dedicated `Value::Bool` variant; `if`/`while` conditions require it |
| `String` | primitive | Dedicated `Value::String` variant; string literals, `+` concatenation hardcoded |

**Operator overloading and its impact on `std::core`.**

RFC-0011 proposes that arithmetic and comparison operators desugar into aspect method calls — `a + b` becomes `Add::add(a, b)`, `a == b` becomes `Eq::eq(a, b)`, and so on. This creates a new class of compiler-special aspects: the operator aspects. Like `Display`, `Iterable`, and `From`, these must always be in scope because the compiler desugars every operator expression into a call to one of them. They belong in `std::core`.

The planned operator aspects (drawing from RFC-0011 and issue #149):

| Aspect | Operator(s) | Note |
|---|---|---|
| `Add<Rhs>` | `+` | Returns `Self` for v0.5.0; associated `Output` type deferred |
| `Sub<Rhs>` | `-` (binary) | |
| `Mul<Rhs>` | `*` | |
| `Div<Rhs>` | `/` | |
| `Rem<Rhs>` | `%` | |
| `Neg` | `-` (unary) | |
| `Not` | `!` (unary) | |
| `Eq` | `==`, `!=` | `!=` derived from `==` |
| `Ord` | `<`, `<=`, `>`, `>=` | Requires `Eq`; ordering expressed as `compare() -> Ordering` |
| `AddAssign<Rhs>` | `+=` | Deferred to post-v0.5.0 unless needed for `for-in` |

`Ordering` (the return type of `Ord::compare`) is also a `std::core` type: `enum Ordering { Less, Equal, Greater }`.

**This also means the primitives belong in `std::core`.**

If `Add::add`, `Eq::eq` etc. live in `std::core`, then `impl Add for Int` must live somewhere with access to both `Add` and `Int`. Keeping `Int` as a compiler built-in with no module path creates a split: the aspect definition is in `std::core`, but the implementation for the most common type has no co-location. Every future numeric type (`Int64`, `Float32`, etc.) would face the same inconsistency.

The cleaner model: `Int`, `Float`, `Bool`, and `String` are declared in `std::core` with their full set of aspect implementations. They remain primitives in the compiler's internal representation — dedicated `Value` variants, special inference rules, `Bool` required by control flow — but they gain a module path. "Has a module path" and "has special compiler treatment" are orthogonal.

**Proposed decision: `std::core` is auto-imported in every file.**

`std::core` contains all types and aspects that the compiler desugars into, plus the primitive types and their implementations. Every Moonlane program behaves as if `use std::core::*;` appears implicitly at the top of every file — the programmer never writes this import. This is the Haskell `Prelude` model.

```moonlane
// std/core.mln — always auto-imported

// Primitive types (compiler-special internally, but module-defined)
pub primitive type Int
pub primitive type Float
pub primitive type Bool
pub primitive type String

// Sum types with compiler-special pattern matching
pub enum Perhaps<T> {
    Some { value: T },
    None,
}

pub enum Result<T, E> {
    Ok  { value: T },
    Err { error: E },
}

// Range types (produced by .. and ..= operators)
pub struct Range          { start: Int, end: Int }
pub struct RangeInclusive { start: Int, end: Int }

// Ordering (return type of Ord::compare)
pub enum Ordering { Less, Equal, Greater }

// I/O and conversion aspects (compiler-dispatched)
pub aspect Display     { fun to_string(self: @Self) -> String }
pub aspect Iterable<T> { fun next(self: Self) -> (Perhaps<T>, Self) }
pub aspect From<Src>   { fun from(src: Src) -> Self }

// Operator aspects (RFC-0011, compiler-desugared)
pub aspect Add<Rhs> { fun add(self: Self, rhs: Rhs) -> Self }
pub aspect Sub<Rhs> { fun sub(self: Self, rhs: Rhs) -> Self }
pub aspect Mul<Rhs> { fun mul(self: Self, rhs: Rhs) -> Self }
pub aspect Div<Rhs> { fun div(self: Self, rhs: Rhs) -> Self }
pub aspect Rem<Rhs> { fun rem(self: Self, rhs: Rhs) -> Self }
pub aspect Neg      { fun neg(self: Self) -> Self }
pub aspect Not      { fun not(self: Self) -> Self }
pub aspect Eq       { fun eq(self: @Self, other: @Self) -> Bool }
pub aspect Ord: Eq  { fun compare(self: @Self, other: @Self) -> Ordering }

// Primitive impls — all co-located with the types
impl Display for Int    { ... }
impl Display for Float  { ... }
impl Display for Bool   { ... }
impl Display for String { ... }

impl From<Float> for Int   { ... }
impl From<Int>   for Float { ... }

impl Add<Int>   for Int   { ... }
impl Sub<Int>   for Int   { ... }
impl Mul<Int>   for Int   { ... }
impl Div<Int>   for Int   { ... }
impl Rem<Int>   for Int   { ... }
impl Neg        for Int   { ... }
impl Add<Float> for Float { ... }
// ... and so on

impl Eq  for Int    { ... }
impl Eq  for Float  { ... }
impl Eq  for Bool   { ... }
impl Eq  for String { ... }
impl Ord for Int    { ... }
impl Ord for Float  { ... }
impl Ord for String { ... }

impl Add<String> for String { ... }   // string concatenation
impl Not         for Bool   { ... }   // boolean negation
```

The `pub primitive type` declaration is a new grammar form — a hint to the compiler that this type has a dedicated internal representation. The compiler still generates dedicated `Value::Int` etc. variants; the declaration just gives the type a module path and a location for its impls.

**Future numeric types.**

When `Int64`, `Float32`, `UInt` etc. are added, they follow the same pattern:
- Default-width types (`Int`, `Float`) stay in `std::core` — always in scope
- Specialised numeric types (`Int64`, `Float32`, `UInt`, etc.) live in `std::numeric` — explicit `use` required

This creates a clear two-tier model: you get `Int` and `Float` for free; you opt in to anything more specific.

**Shadowing rule.** A user-defined name that matches a `std::core` export shadows the auto-import in the declaring module only. Consistent with how `let` bindings shadow outer scope names in expressions.

```moonlane
enum Perhaps<T> { Some(T), Empty }   // shadows std::core::Perhaps in this file only
fun check(x: Perhaps<Int>) -> Bool { ... }   // refers to the local Perhaps
```

An explicit `use std::core::Perhaps;` in the same file as a local definition of `Perhaps` is a name conflict (OQ-3) and is an error.

**Interaction with OQ-3 (name conflicts).** The auto-import is lowest priority: any explicit `use` declaration beats it without raising a conflict. A conflict is only raised between two explicit `use` declarations that bind the same name.

---

### OQ-9 — `mod` and `use` ordering within a file

RFC-0009 states that "all `use` statements must appear at the top level of a file, before any declarations." A `mod name;` statement is also top-level. The ordering rule is ambiguous.

**Decision:** The natural reading consistent with Rust's convention is:

```
file ::= mod-decl* use-decl* declaration*
```

`mod` declarations come first, then `use` statements, then all other declarations. This makes `mod` declarations effectively part of the file header alongside `use`. The compiler resolves all `mod` paths before processing `use` statements, so forward references between `use` and `mod` are not an issue.

This is a minor clarification, not a design choice — but it must be stated explicitly in the spec.

---

### OQ-10 — `super::` and relative paths (deferred)

RFC-0009 explicitly defers `super::` and `self::` to a future version. This is acknowledged as a known ergonomic gap: a submodule that needs to reference a sibling must write `crate::sibling::Name` rather than `super::sibling::Name`.

No decision is required now. This is recorded here so the v0.5.0 implementation does not accidentally make relative paths harder to add later. Specifically: the path resolution algorithm should be written to accept `super` and `self` as path roots even if they are currently rejected with "not yet supported", rather than treating them as user-defined module names.

---

## Decision

**Outcome:** *(pending)*  
**Target:** v0.5.0

*(Decisions to be recorded here after review.)*
