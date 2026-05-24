---
id: rfc-0002
title: "Trait Bound Syntax"
date: '2026-05-19'
status: accepted
---

## Summary

Define the syntax for expressing trait bounds on generic type parameters. The current spec sketches Rust-style syntax (`T: Trait`, `where` clause) without fully specifying multi-bound forms, anonymous type parameters, or associated type constraints. This RFC evaluates alternatives from other languages and proposes a design that is expressive, readable, and internally consistent with Moonlane's existing syntax.

---

## The Problem with Rust

Rust's trait bound syntax has several friction points that compound as code grows:

### 1. The `+` separator reads like addition

```rust
fn foo<T: Display + Clone + Debug>(x: T)
```

`+` is the arithmetic addition operator. In a type constraint position it means "and also implements", but there's no visual connection to that meaning. Readers new to Rust routinely misread this.

### 2. Inline bounds vs `where` creates a dual-syntax problem

```rust
// Form 1 — inline
fn foo<T: Display + Clone>(x: T) -> T

// Form 2 — where clause (identical semantics)
fn foo<T>(x: T) -> T where T: Display + Clone
```

Both are legal. Neither is deprecated. Projects develop their own conventions. When bounds grow long, inline becomes unreadable and you migrate to `where`, but now the type parameter declaration (`<T>`) and its constraints (`where T: ...`) are separated by the entire parameter list and return type.

### 3. `impl Trait` and `<T: Trait>` are two ways to say the same thing

```rust
fn bar(x: impl Display) -> impl Display        // anonymous type param
fn bar<T: Display>(x: T) -> T                  // explicit type param
```

Both compile to the same monomorphized code for parameters. They have subtle differences at return position (`impl Trait` in return is opaque; `T` in return is named). This duality forces every Rust programmer to learn two mental models for one concept.

### 4. Associated type syntax is unusual

```rust
fn process<T: Iterator<Item = String>>(iter: T)
```

`Item = String` inside angle brackets looks like a named argument, not a type constraint. It's a different syntactic form from everything else in the bound.

### 5. Repetition under complex bounds

```rust
fn foo<T, U, V>(x: T, y: U, z: V) -> T
where
    T: Display + Clone + Eq,
    U: Iterator<Item = T> + Send,
    V: From<T> + Into<U>,
```

The type parameters are declared in `<T, U, V>`, then constrained in `where`. Any rename touches two sites. Any reordering touches two sites.

---

## How Other Languages Do It

### TypeScript

```typescript
// Single bound — `extends` keyword
function largest<T extends Comparable>(a: T, b: T): T

// Multiple bounds — intersection type with `&`
function foo<T extends Displayable & Cloneable>(x: T): T

// Inline intersection at parameter position (anonymous)
function bar(x: Displayable & Cloneable): Displayable & Cloneable
```

**Verdict:** `extends` reads naturally (T must be a subtype / implementor). `&` for intersection is clean. The duality between named `<T extends ...>` and anonymous intersection types at parameter position is a small wart.

---

### Swift

```swift
// Single bound
func largest<T: Comparable>(_ a: T, _ b: T) -> T

// Multiple bounds — protocol composition with `&`
func foo<T: Displayable & Cloneable>(_ x: T) -> T

// Anonymous type params — `some` keyword (opaque types)
func bar(_ x: some Displayable & Cloneable) -> some Displayable

// Where clause for conditional conformance
func baz<T, U>(_ x: T, _ y: U) where T: Displayable, T == U

// Primary associated types (Swift 5.7) — clean associated type syntax
func process(_ iter: some Collection<String>)
```

**Verdict:** Very clean. `some` for anonymous params is readable and distinct. `&` for protocol composition is intuitive. The `some` / explicit-param duality still exists but is at least visually distinct. The primary associated type syntax (`Collection<String>` meaning `Collection<Element = String>`) eliminates Rust's `Item = String` awkwardness.

---

### Haskell

```haskell
-- Single constraint
largest :: Ord a => a -> a -> a

-- Multiple constraints — comma-separated in parentheses
foo :: (Show a, Eq a, Ord a) => a -> String

-- Multi-param type classes
bar :: Convertible a b => a -> b
```

**Verdict:** Extremely terse. Constraints are grouped before the type signature with `=>`. Reading right-to-left (`a -> a -> a` given that `a` satisfies `Ord`) takes adjustment, but once internalized, the separation of constraints from parameter types is clear. The downside: constraint inference is so powerful in Haskell that you rarely write bounds at all — the ergonomic pressure in Moonlane will be different.

---

### Kotlin

```kotlin
// Single inline bound
fun <T : Comparable<T>> largest(a: T, b: T): T

// Multiple bounds — where clause only (no inline multi-bound)
fun <T> foo(x: T): T where T : Displayable, T : Cloneable

// Intersection type at use site
fun bar(x: Displayable & Cloneable): Displayable & Cloneable
```

**Verdict:** `:` for single bounds is clean. Multiple bounds require the `where` clause — no inline form, which eliminates the dual-syntax problem at the cost of always writing `where` for multiple constraints. The `&` intersection at use site (for anonymous/concrete intersection) is consistent.

---

### Go

```go
// Constraint as interface
func Largest[T constraints.Ordered](a, b T) T

// Inline anonymous interface as constraint
func Foo[T interface{ String() string; Clone() T }](x T) T

// Type union in constraint (type set model)
type Number interface { int | float64 }
func Sum[T Number](nums []T) T
```

**Verdict:** The type-set model (constraints as interface unions) is elegant for numeric constraints but unfamiliar. Inline anonymous interfaces as constraints are verbose. The unification of "interface" and "constraint" is intellectually clean but erases the distinction between "this type has these methods" and "this type is one of these concrete types."

---

### Scala 3

```scala
// Context bound (single) — terse but implicit
def largest[T: Ordering](a: T, b: T): T

// Explicit using (multiple)
def foo[T](x: T)(using Displayable[T], Cloneable[T]): T

// Intersection type
def bar(x: Displayable & Cloneable): Displayable & Cloneable

// Where clause (for type equalities)
def baz[T, U](x: T)(using T =:= U): U
```

**Verdict:** Context bounds (`[T: Ordering]`) are extremely concise but hide the constraint entirely — you can't tell what `Ordering` requires without looking it up. The `using` clause is explicit and composable. Intersection types at use site are clean.

---

## Design Options for Moonlane

The current spec sketches:

```moonlane
fun largest<T: Comparable>(a: T, b: T) -> T where T: Comparable { ... }  // inline
fun largest<T: Comparable>(a: T, b: T) -> T { ... }                       // also inline
```

`where` is already a keyword. `GenericParam` in the AST has one optional bound (`bound: Option<TypeExpr>`). Neither multiple bounds nor anonymous type params are currently specified.

---

### Option A: Rust-like — minimal change

Extend the current spec with `+` for multiple bounds. Inline and `where` both supported.

```moonlane
fun foo<T: Display + Clone>(x: T) -> T

fun foo<T, U>(x: T, y: U) -> T
    where T: Display + Clone,
          U: Iterable<T>
```

**Pros:** Familiar to Rust users. Minimal AST change (bound becomes `Vec<TypeExpr>`).  
**Cons:** Inherits all of Rust's friction. `+` looks like arithmetic.

---

### Option B: `&` for trait composition

Replace `+` with `&` as the multi-bound separator. `&` has precedent in TypeScript, Swift, and Scala for type intersection / protocol composition.

```moonlane
fun foo<T: Display & Clone>(x: T) -> T

fun bar<T, U>(x: T, y: U) -> T
    where T: Display & Clone,
          U: Iterable<T>
```

**⚠ Interaction with RFC-0001:** `&` is the address-of operator in expressions (`&x`, `&mut x`). In type/constraint positions, `*T` is a pointer type and `&` does not appear as a standalone token. The overlap is syntactically unambiguous but visually noisy — a programmer who reads `T: Display & Clone` may wonder if `&` means something pointer-related.

**Pros:** Cleaner than `+`. `&` for "and also" is readable.  
**Cons:** Visual tension with RFC-0001 pointer operator.

---

### Option C: `where`-first, comma-separated, drop inline multi-bound

Adopt Kotlin's approach: `:` for single inline bounds, `where` clause (mandatory) for multiple. This eliminates the dual-syntax problem entirely.

```moonlane
// Single bound — inline allowed
fun largest<T: Comparable>(a: T, b: T) -> T

// Multiple bounds — where clause only
fun foo<T, U>(x: T, y: U) -> T
    where T: Display, T: Clone,
          U: Iterable<T>

// Anonymous type param (single bound only)
fun bar(x: impl Display) -> impl Display
```

**Pros:** One canonical place for multi-bound constraints. No `+` / `&` decision. Comma-separated per-trait reads as a list.  
**Cons:** Multi-bound functions always require a `where` clause; more vertical space. `T: Display, T: Clone` repeats `T`.

---

### Option D: `requires` clause with implicit type params

Introduce `requires` as a constraint clause. Type parameters are inferred from usage — you don't declare `<T>` separately unless you need to name the type in the return position.

```moonlane
// Type param declared only when needed in return type
fun largest(a: T, b: T) -> T requires T: Comparable

// Named for complex multi-constraint cases
fun foo(x: T, y: U) -> T requires T: Display & Clone, U: Iterable<T>

// No redundancy: type params emerge from the `requires` clause
fun sort(arr: T[]) requires T: Comparable
```

**Pros:** Eliminates `<T>` declaration entirely for simple cases. Constraint is always co-located with the signature. No dual-syntax problem — there is only `requires`.  
**Cons:** New keyword. Type params are implicit, which may reduce readability for complex multi-param functions. `requires` after the return type is a new syntactic position not found in the spec today.

---

### Option E: Constraint block — explicit grouping

Inspired by Haskell's `=>` and Scala's `using`. Move all bounds into a named block, always after the signature:

```moonlane
fun foo(x: T, y: U) -> T
    [T: Display + Clone, U: Iterable<T>]

// Or with explicit where:
fun foo(x: T, y: U) -> T
    where [T: Display + Clone, U: Iterable<T>]
```

**Pros:** Type params and their constraints are grouped together, eliminating the `<T>` / `where T:` split.  
**Cons:** Another syntactic form to learn. `[]` is already used for array types (`T[]`).

---

## AST Impact

The current `GenericParam` is:

```rust
pub struct GenericParam {
    pub name:  String,
    pub bound: Option<TypeExpr>,   // single bound only
}
```

All options require changing `bound` to support multiple traits. The least-invasive change:

```rust
pub struct GenericParam {
    pub name:   String,
    pub bounds: Vec<TypeExpr>,     // empty = unconstrained
}
```

`where` clauses would be stored separately (already partially implied by the existing `where` keyword in the grammar):

```rust
pub struct WhereClause {
    pub constraints: Vec<(String, Vec<TypeExpr>)>,   // (type_param_name, [bound, ...])
}
```

Options C and D additionally require:
- Implicit type param extraction (Option D): the parser must collect all unconstrained identifier-shaped type expressions and treat them as type params
- `requires` clause (Option D): new AST node alongside `where`

---

## Open Questions

1. **Which separator for multiple bounds?**  
   `+` (Rust, familiar), `&` (Swift/TypeScript, clean but RFC-0001 tension), `,` (comma list in `where`), or none if multi-bounds are `where`-only.

2. **Should inline multi-bound be allowed at all?**  
   Kotlin bans it; everything goes in `where`. Rust allows both. Allowing both creates the dual-syntax problem. If inline multi-bound is forbidden, bounds on a single-constraint param can be inline (`<T: Comparable>`) but two+ bounds always require `where`.

3. **Anonymous type params — `impl Trait` or something else?**  
   `impl Trait` for parameter position is a known Rust tension (why `impl` — what does it implement?). Swift's `some` is semantically clear. Should Moonlane introduce a keyword, or require explicit `<T: Trait>` for all generic positions?

4. **Associated type constraints — how to express them?**  
   Rust: `Iterator<Item = String>`. Swift: `Collection<String>` (primary associated type). Moonlane's `Iterable<T>` could adopt Swift's model — `Iterable<String>` constrains the element type directly without named-argument syntax.  
   This is the cleanest option and is consistent with how `Perhaps<T>` and `Result<T, E>` are already written.

5. **`where` vs `requires` — keep one or both?**  
   `where` is already a keyword. `requires` reads more declaratively ("this function requires...") and is less likely to be confused with the SQL/query connotation of `where`. If both are allowed, another dual-syntax problem emerges.

6. **Self-referential bounds — `T: Comparable<T>` vs `T: Comparable`?**  
   Rust's `PartialOrd<Rhs = Self>` is complex. Kotlin's `Comparable<T>` is explicit but repetitive. The Moonlane spec already shows `Comparable` without a self-type parameter. This should be codified: bounds use the simple name (`Comparable`), and `Self` in the trait definition refers to the implementing type. This avoids `T: Comparable<T>` entirely.

7. **Type aliases for constraint bundles — `type` or `trait` alias?**

   Type aliases let you name a compound bound and use it as a single inline constraint, directly relieving Option C's main ergonomic cost:

   ```moonlane
   type Sortable = Comparable & Display & Clone   // defined once
   
   fun sort<T: Sortable>(arr: T[]) -> T[]         // single inline bound — no where needed
   fun display_all<T: Sortable>(items: T[])        // reused across functions
   ```

   The alias definition is where you pay the verbosity cost; every call site stays clean. This mirrors Swift's `typealias` for protocol compositions.

   **The catch:** alias definitions cannot use Option C's `where`-style comma-per-trait form (there is no `T` to repeat). They need a conjunction operator in the definition — which reintroduces the separator question that Option C sidesteps at call sites:

   ```moonlane
   type Sortable = Comparable & Display & Clone   // & separator in definition
   type Sortable = Comparable + Display + Clone   // + separator in definition
   ```

   Alternatively, a separate `trait alias` keyword scopes the separator decision to a distinct syntactic form, making it visually unambiguous:

   ```moonlane
   trait Sortable = Comparable + Display + Clone  // trait alias — not a type alias
   ```

   This matters because a `type` alias is broadly useful beyond constraints (aliasing function signatures, generic instantiations, etc.), while a `trait alias` is specifically for bundling bounds. The two may warrant different syntax.

   **Sub-questions:**
   - Is a constraint alias a `type` alias or a `trait` alias (or both)?
   - Does the conjunction separator in alias definitions need to match the separator used at call sites (if any)?
   - Should constraint aliases be usable anywhere a trait bound appears, including `where` clauses?

8. **Forward reference — async/concurrency trait bounds**  
   If Moonlane ever gains async or multithreading support, the trait system will need `Send`/`Sync`-like marker traits to express thread-safety constraints, and the pointer RFC (`*mut T`) will need corresponding bounds to prevent unsound shared mutable access across threads. No design is needed now — defer until after the evaluator PoC and traits implementation give enough implementation experience to make concrete choices. Noted here to avoid designing the trait system into a corner.

---

## Recommendation

**Adopt Option C (where-first, comma-separated) with Swift's associated type model for open question 4, and introduce both `type` aliases (for concrete types and generic instantiations) and `aspect` aliases (for constraint bundles).**

Rationale:
- Eliminating inline multi-bound removes the dual-syntax problem permanently
- `where T: Display, T: Clone` is verbose but unambiguous — each constraint is a standalone statement
- No new separator character needed at call sites (no `+` / `&` decision for the common case)
- `aspect Sortable = Comparable + Display` localises the separator decision to alias definitions, where a conjunction operator is unavoidable; `+` is acceptable there because it is a definition form, not a usage form
- Constraint aliases give authors an ergonomic escape from long `where` clauses for repeated bound combinations, without compromising the call-site clarity that Option C provides
- Consistent with `where` already being a keyword
- Associated type as `Iterable<String>` rather than `Iterator<Item = String>` is simpler and consistent with `Perhaps<T>`, `Result<T, E>` conventions already in the spec
- `Self` inside an aspect definition refers to the implementing type; call sites use the bare aspect name with no type parameter repetition — consistent with Rust and Swift, and avoids Java/Kotlin-style F-bounded verbosity (`T: Comparable<T>`)
- `impl Aspect` syntax adopted for anonymous type parameters

---

## References

- Language spec: [`spec/declarations.md#traits`](../../public/spec/declarations.md#traits), [`spec/types.md#generics`](../../public/spec/types.md#generics)
- RFC-0001: `docs/internal/rfcs/rfc-0001-pointer-syntax.md` (`&` operator — tension with Option B)
- v0.3: #5–#10 (type variables, generics, monomorphization)
- v0.3: #11–#13 (traits and method dispatch)
- AST: `src/ast/mod.rs` — `GenericParam`, `TraitDecl`, `TraitMethod`

---

## Decision

**Outcome:** Accepted  
**Target:** v0.2

Resolved questions:

| # | Question | Decision |
|---|---|---|
| 1 | Separator for multiple bounds | Option C — no inline multi-bound; `where` clause, one bound per line |
| 2 | Inline multi-bound vs `where`-only | `where`-only for multiple bounds; single bound may be inline |
| 3 | Anonymous type parameters | `impl Aspect` syntax |
| 4 | Associated type constraints | Swift-style primary associated type — `Iterable<String>`, not `Iterable<Item = String>` |
| 5 | `where` vs `requires` | `where` keyword |
| 6 | Self-referential bounds | Implicit `Self` — aspects define `Self`, call sites use bare aspect name |
| 7 | Constraint bundles | `aspect` alias — `aspect Sortable = Comparable + Display + Clone` |
| 8 | Async/concurrency bounds | Deferred — no action until concurrency model is designed |

The combination of `where`-only multi-bounds, implicit Self, and `aspect` aliases gives a clean, unambiguous syntax without any of Rust's dual-syntax problems or Java/Kotlin's F-bounded verbosity. The `+` separator is confined to `aspect` alias definitions, where a conjunction operator is unavoidable — it does not appear at call sites.
