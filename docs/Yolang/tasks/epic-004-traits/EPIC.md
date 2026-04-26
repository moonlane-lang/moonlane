# Epic 004: Traits and Method Dispatch

**Status:** planning (do not start until Epics 001-003 complete)  
**Created:** 2026-04-26  
**Depends On:** Epic 003 (Generics and Monomorphization)

## Overview

Implement **traits** — Rust-style abstractions for defining shared behavior across types. Traits define interfaces, can be implemented by structs/enums, and enable polymorphism through method dispatch.

This epic is a major language feature that depends on having a working type system and evaluator. **Do NOT start until Epics 001-003 are complete.**

## What Are Traits?

A trait is a set of methods that a type can implement:

```
trait Clone {
    fn clone(&self) -> Self;
}

impl Clone for Int {
    fn clone(&self) -> Int { *self }
}

fn duplicate<T: Clone>(x: T) -> (T, T) {
    (x.clone(), x.clone())
}
```

**Key concepts:**
- **Trait definition** — methods that any type can implement
- **Impl block** — a type's implementation of a trait
- **Trait bounds** — generic constraints (`T: Clone` means "T must implement Clone")
- **Method dispatch** — which implementation to call
  - Static: at compile time (monomorphization, like Rust)
  - Dynamic: at runtime (vtables, like Rust's `dyn Trait`)

## Goals

1. **Trait Definitions** — Parse and type-check trait declarations
2. **Trait Implementations** — Type-check `impl Trait for Type`
3. **Trait Bounds** — Support generic constraints (`<T: Clone>`)
4. **Static Dispatch** — Monomorphize generic code with trait bounds
5. **Method Resolution** — Find correct method for a type
6. **Trait Objects** — Support `dyn Trait` for runtime polymorphism (stretch goal)

## Why Traits?

- **Code reuse** — Define once, implement for many types
- **Polymorphism** — Call methods on trait objects, dispatch at runtime
- **Generic constraints** — Express "this function works for any type implementing X"
- **Standard library** — Collections, iterators, etc. require traits
- **Composability** — Mix and match implementations

## Critical Design Decisions (MUST DECIDE BEFORE IMPLEMENTATION)

### 1. Static vs. Dynamic Dispatch

**Option A: Static dispatch only (Rust's default)**
- Monomorphize all trait calls
- No `dyn Trait` objects
- More code bloat, better performance
- Simpler to implement

**Option B: Dynamic dispatch only**
- Vtables for all trait calls
- Simpler implementation
- More runtime overhead
- May be harder to reason about

**Option C: Both (Rust's approach)**
- Static for `fn foo<T: Clone>()` (monomorphized)
- Dynamic for `&dyn Clone` (vtable dispatch)
- Most flexible, most complex to implement

**RECOMMENDATION:** Start with **Option A (static only)**. Add dynamic dispatch later if needed.

### 2. Method Receivers

How do methods receive `self`?

```
trait Clone {
    fn clone(&self) -> Self;       // Borrow self
    fn consume(self) -> String;    // Take ownership
    fn mutate(&mut self);          // Mutable borrow
}
```

**Decision:** Support all three receiver types (`&self`, `self`, `&mut self`). Type checker already knows borrowing rules.

### 3. Trait Bounds Syntax

Decide syntax for multiple bounds:

```
fn foo<T: Clone + Display>(x: T) { ... }    // Option A: + syntax
fn foo<T: Clone, T: Display>(x: T) { ... }  // Option B: repetition
fn foo<T>(x: T) where T: Clone + Display { ... }  // Option C: where clause
```

**RECOMMENDATION:** Use `+` syntax (Rust-like). Where clauses are complex, defer.

### 4. Default Methods

Should traits support default implementations?

```
trait Clone {
    fn clone(&self) -> Self;           // Required
    fn cloned(&self) -> Vec<Self> {    // Default impl
        vec![self.clone()]
    }
}
```

**RECOMMENDATION:** Yes, but as a stretch goal. Required methods first.

## Architecture

### Phase 1: Trait Type Checking

```
Trait Definition (parsed) → Validate methods → Store in trait table
Type Definition → Check impl implements all methods → Store in impl table
Generic constraints → Validate type implements trait → Add to type info
```

### Phase 2: Method Resolution

```
Method call `obj.method(args)` → Look up trait impl for obj's type → Call implementation
Generic call `foo::<ConcreteType>()` with trait bounds → Monomorphize with concrete impl
```

### Phase 3: Dynamic Dispatch (LATER)

```
Trait object `&dyn Trait` → Vtable dispatch at runtime
```

## Out of Scope (for Epic 004)

- **Associated types** (`trait Iter { type Item; }`)
- **Where clauses** (defer to later)
- **Higher-ranked trait bounds** (`for<'a>`)
- **Negative traits** (`trait NotClone`)
- **Trait inheritance** (traits extending other traits)
- **Async traits** (depends on async support)
- **Coherence checking** (prevent overlapping impls — complex, defer)

## Success Criteria

When this epic is done:

- [ ] Trait definitions parse and type-check
- [ ] Trait implementations parse and type-check
- [ ] Trait methods can be called on concrete types
- [ ] Trait bounds work on generic types
- [ ] Monomorphization respects trait bounds
- [ ] Method dispatch finds correct implementation
- [ ] Type errors clearly report missing trait impls
- [ ] Tests cover trait definitions, impls, bounds, method calls
- [ ] All Epics 1-3 tests still pass
- [ ] Standard library uses traits (e.g., `Clone`, `Display`)

## Estimated Subtasks

These are **placeholders** — create detailed tasks only when ready to implement.

1. **Trait Definition Type Checking**
   - Parse trait methods (signatures, receivers)
   - Store trait info in type table
   - Validate method names don't conflict

2. **Trait Implementation Checking**
   - Type-check `impl Trait for Type`
   - Verify all required methods implemented
   - Store impl info in type table

3. **Trait Bounds in Generics**
   - Parse `<T: Trait>` syntax
   - Validate type arguments implement required traits
   - Propagate bounds through generic monomorphization

4. **Method Resolution & Dispatch**
   - Resolve `obj.method()` calls to trait impls
   - Generate dispatch code
   - Handle method call evaluation

5. **Trait Objects** (STRETCH GOAL)
   - Support `dyn Trait` syntax
   - Implement vtable dispatch
   - Handle upcasting and downcasting

## Technical Challenges

1. **Coherence** — Prevent overlapping implementations
   - Example: `impl Trait for T` and `impl Trait for T` (conflict!)
   - Defer to Phase 2; use simple "first-match" initially

2. **Circular Dependencies** — Traits can reference other traits
   - Example: `impl Clone for Box<T> where T: Clone`
   - Need proper ordering of type checking

3. **Lifetime Bounds** (if Yolang has explicit lifetimes)
   - Traits can have lifetime parameters
   - Defer to later

4. **Specialized Impls** — Generic impls and concrete impls coexist
   - Example: `impl<T: Clone> Clone for Vec<T>` AND `impl Clone for String`
   - Need overlap resolution

## Integration Points

- **Type Checker (Epic 001):** Already has type table; add trait table
- **Generic Monomorphization (Epic 003):** Respect trait bounds when specializing
- **Evaluator (Epic 002):** Call methods through dispatch mechanism
- **Parser:** Already parses trait syntax

## Notes

- **Don't start until Epics 1-3 are done** — Traits depend on solid type system and evaluator
- **Static dispatch is simpler** — Start there, add dynamic later
- **Test thoroughly** — Traits interact with generics, borrowing, and method calls in complex ways
- **Coherence is hard** — Defer complex coherence rules to Phase 2
- **Standard library needs traits** — But can ship v0.1 without traits if needed

## Future Considerations

After Epic 004, consider:
- **Associated types** — `trait Iterator { type Item; }`
- **Higher-ranked bounds** — `for<'a>`
- **Negative traits** — `trait NotClone` (to support auto traits)
- **Async traits** — Traits for async functions
- **Const traits** — Traits for compile-time code
- **GATs** — Generic Associated Types
