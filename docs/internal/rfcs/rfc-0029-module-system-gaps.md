---
id: rfc-0029
title: "Module System — Gaps and Clarifications"
date: '2026-05-27'
status: superseded
superseded-by: rfc-0030
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

mod ast;       // internal — callers should not import root::parser::ast directly
pub mod lexer; // public — root::parser::lexer is part of the API
```

**Options:**

- **Option A — `pub mod` / `mod` distinction (Rust-style).** A bare `mod name;` declares a private submodule — it exists and is accessible within the declaring module, but `root::parser::ast::*` is not a valid path for external callers. `pub mod name;` makes the submodule publicly reachable. `pub use` is still needed to re-export individual names from a private submodule.
- **Option B — All declared modules are implicitly public.** A `mod name;` declaration always makes `root::…::name` reachable from outside. Module privacy is controlled entirely by `pub` on the individual declarations inside the module, not on the module itself. Simpler surface, but no way to hide an entire internal submodule without qualifying every item.
- **Option C — All declared modules are implicitly private; `pub use` is the only export path.** You can never import `root::parser::ast::Ast` directly; you must go through re-exports declared in a `pub use` chain. Maximum encapsulation, but verbose for straightforward hierarchies.

**Decision:** Accept **Option A**. A bare `mod name;` declares a private submodule. `pub mod name;` makes the submodule publicly reachable.

This matches RFC-0009's existing `pub use` example, where an internal submodule can remain hidden while selected names are re-exported through the parent module.

Grammar impact:

```
mod-decl ::= 'mod' identifier ';'
           | 'pub' 'mod' identifier ';'
```

---

### OQ-2 — root path definition

RFC-0009 says `crate` refers to "the file containing the entry point" but does not define what that file is or how it is determined. This RFC also rejects the `crate` spelling in favour of `root::`.

Two cases need a rule:

1. **Binary programs** (have `main()`): which file is the root? The file passed directly to the compiler? Always `main.mln`? A project manifest?
2. **Library modules** (no `main()`): what file is `root::` rooted at? How does a caller of the library address its root?

**Options:**

- **Option A — Root is always the file passed to the compiler CLI.** `moonlane run src/main.mln` makes `src/main.mln` the root module. Libraries are compiled with `moonlane build src/lib.mln`. Simple, explicit, no manifest needed.
- **Option B — Root is always a fixed filename.** Binary: `main.mln`. Library: `lib.mln`. The compiler looks for these names in the source root. Predictable, convention-based.
- **Option C — Project manifest (e.g. `moonlane.toml`) declares the entry point.** The compiler reads the manifest to find the root file. More infrastructure to define now but necessary for multi-target projects (binary + library in one project).

**Decision:** Accept **Option A** for v0.5.0, with the root path spelled `root::` instead of `crate::`. In script mode, the root module is the file passed directly to the toolchain. `moonlane run src/main.mln` roots `root::` at `src/main.mln`.

This preserves single-file interpreted programs and also supports interpreted multi-file scripts: `mod` declarations are resolved relative to the selected root file and its submodules. A future manifest-based project mode can still select an explicit root file, then reuse the same module-resolution rules.

---

### OQ-3 — Name conflicts from multiple imports

When two `use` statements bring the same identifier into scope, the behaviour is undefined in RFC-0009.

```moonlane
use root::parser::Token;
use root::lexer::Token;   // conflict — what happens?
```

**Options:**

- **Option A — Compile error.** Any two `use` statements that would bind the same name in the current scope are rejected, regardless of whether the names refer to the same item or different items.
- **Option B — Compile error only when the name is actually used.** The conflict is reported at the use site, not at the `use` statement. Allows importing conflicting names as long as only one is referenced.
- **Option C — Last declaration wins.** The later `use` shadows the earlier one silently. Consistent with how local `let` bindings shadow, but surprising at the import level.

**Decision:** Accept **Option A** for explicit imports. If two explicit `use` declarations bind the same local name in the same module, the program is rejected at the second import.

Glob imports (`use path::*;`) use a softer ambiguity rule:

- Explicit imports beat glob imports.
- Local declarations beat glob imports.
- Two glob imports may export the same name without an immediate error.
- A name from conflicting glob imports is an error only if it is referenced ambiguously.

This keeps deliberate imports deterministic while avoiding fragile glob imports that break merely because an upstream module added a new public name.

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
use root::v1::Parser;
use root::v2::Parser;   // conflict — no way to use both
```

Without aliasing, the only workaround is to not import one name and use its full path inline — but RFC-0009 also does not define whether qualified paths are valid in expression and type position without a `use` declaration.

**Two sub-questions:**

**OQ-5a — `use … as` aliasing.** Should `use path::to::Name as Alias` be valid in v0.5.0?

- **Option A — Yes, ship aliasing in v0.5.0.** Necessary for any program that uses two modules exporting the same name. Without it, name conflicts are entirely unresolvable.
- **Option B — Defer; require full paths for conflict resolution.** Only viable if OQ-5b is resolved in favour of allowing inline qualified paths.

**OQ-5b — Inline qualified paths without `use`.** Should `root::parser::Ast` be valid in type and expression position without a corresponding `use` declaration?

- **Option A — Yes.** Any fully-qualified path is valid anywhere a name is expected. `use` is syntactic sugar for bringing a name into the local scope, not the only way to access an item.
- **Option B — No.** Items are only accessible by their short name after a `use` declaration. Full paths in expression position are not valid syntax.

These two questions interact: if both are "no" and "no", name conflicts are entirely unresolvable. At least one must be "yes."

**Decision:** Accept **Option A** for both sub-questions.

`use path::to::Name as Alias;` is valid in v0.5.0 and binds `Alias` in the current module. Aliasing is valid for single imports and grouped imports:

```moonlane
use root::v1::Parser as ParserV1;
use root::v2::{Parser as ParserV2, Token};
```

Fully-qualified paths are also valid in expression and type position without a preceding `use` declaration:

```moonlane
let parser: root::parser::Parser = root::parser::Parser::new();
```

`use` is therefore a local binding convenience, not the only mechanism for accessing a public item.

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

**Decision:** Accept **Option A** for v0.5.0. Fields of a `pub struct` are public. Fields of a private struct are private because the struct itself is not externally nameable.

Field-level visibility remains desirable, but it is deferred to a follow-up issue for Option B (`pub` per field). That work must define how private fields interact with struct literals, pattern matching, and constructor functions.

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

**Decision:** Accept **Option A**. `use path::module;` brings the module into scope as a path handle. After `use std::math;`, `math::sin(1.0)` is valid.

Importing individual items remains valid and is still preferred when only a small number of names are needed:

```moonlane
use std::collections;
use std::collections::{Map, Set};
```

Both declarations are legal if they bind different local names (`collections`, `Map`, and `Set`).

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

RFC-0011 proposes that arithmetic and comparison operators desugar into aspect method calls. The desugaring follows Moonlane's existing dispatch model — the receiver type comes first, the aspect and type parameter second. This is already the pattern the evaluator uses for `From`: `?` on a `Result<T, E>` resolves to `TargetType::From<E>::from`, not `From::from`. Operator desugaring is the same:

```
a + b   (a: Int, b: Int)   →   Int::Add<Int>::add(a, b)
a == b  (a: Int, b: Int)   →   Int::Eq::eq(a, b)
a < b   (a: Int, b: Int)   →   Int::Ord::compare(a, b) == Ordering::Less
```

The aspect definition in `std::core` is the interface contract. The runtime lookup key is `TypeA::Aspect<TypeB>::method`. This creates a new class of compiler-special aspects — like `Display`, `Iterable`, and `From`, the operator aspects must always be in scope once operator desugaring is implemented because the compiler desugars operator expressions into calls to them. They belong in `std::core`, but implementing operator aspect dispatch remains scoped to RFC-0011 / issue #149.

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

**Decision: `std::core` is auto-imported in every file.**

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
// Dispatch: `a + b` (a: T, b: U) → T::Add<U>::add(a, b)
pub aspect Add<Rhs> { fun add(self: Self, rhs: Rhs) -> Self }
pub aspect Sub<Rhs> { fun sub(self: Self, rhs: Rhs) -> Self }
pub aspect Mul<Rhs> { fun mul(self: Self, rhs: Rhs) -> Self }
pub aspect Div<Rhs> { fun div(self: Self, rhs: Rhs) -> Self }
pub aspect Rem<Rhs> { fun rem(self: Self, rhs: Rhs) -> Self }
pub aspect Neg      { fun neg(self: Self) -> Self }           // -a → T::Neg::neg(a)
pub aspect Not      { fun not(self: Self) -> Self }           // !a → T::Not::not(a)
pub aspect Eq       { fun eq(self: @Self, other: @Self) -> Bool }   // a == b → T::Eq::eq(a, b)
pub aspect Ord: Eq  { fun compare(self: @Self, other: @Self) -> Ordering }  // T::Ord::compare

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

### OQ-10 — `super::`, `self::`, and relative paths

RFC-0009 explicitly defers `super::` and `self::` to a future version. This is acknowledged as a known ergonomic gap: a submodule that needs to reference a sibling must write an absolute root path rather than `super::sibling::Name`.

**Decision:** Include `self::` and `super::` in v0.5.0.

Path roots are:

| Root | Meaning |
|---|---|
| `root::` | The selected root module for the current program or package |
| `std::` | The bundled standard library root |
| `self::` | The current module |
| `super::` | The parent module; invalid from the root module |
| imported module name | A module handle introduced by `use path::module;` |
| declared child module name | A child module declared by `mod name;` in the current module |

Relative paths obey the same visibility rules as absolute paths. `super::private_child::Name` is only valid when the referenced module and item are visible from the current module.

---

## Decision

> **Superseded by RFC-0030** (2026-05-28). All open questions resolved here are re-answered in RFC-0030 under the revised `import`/`export` design. See RFC-0030 for the accepted design.

**Outcome:** Accepted — v0.5.0 *(superseded before implementation)*  
**Target:** v0.5.0

| Question | Decision |
|---|---|
| OQ-1 — Module visibility | `mod` is private; `pub mod` makes the submodule path public |
| OQ-2 — Root path | The root path is spelled `root::`; in script mode it points at the file passed to the toolchain |
| OQ-3 — Import conflicts | Explicit import conflicts are immediate errors; glob conflicts are errors only on ambiguous use |
| OQ-4 — Ambiguous `mod` resolution | `name.mln` and `name/mod.mln` existing together is a compile error |
| OQ-5 — Aliasing and qualified paths | `use … as` is supported; fully-qualified paths are valid without `use` |
| OQ-6 — Struct field visibility | Fields of a `pub struct` are public in v0.5.0; field-level visibility is tracked separately |
| OQ-7 — `use module` semantics | `use path::module` brings a module handle into scope |
| OQ-8 — `std::core` | `std::core` contains compiler-special core types/aspects and is auto-imported in every file |
| OQ-9 — File ordering | `mod*`, then `use*`, then declarations |
| OQ-10 — Relative paths | `self::` and `super::` are included in v0.5.0 |
