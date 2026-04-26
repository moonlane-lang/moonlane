# Epic 002: Generics and Monomorphization

**Status:** open  
**Started:** 2026-04-26  
**Depends On:** Epic 001 (Typechecker and Typed AST)

## Overview

Add **generic type support** to the type system, enabling parameterized functions and types. Implement **monomorphization** to specialize generic code for concrete types at compile time.

This epic builds on the foundation of Epic 001's basic type system.

## Goals

1. **Type Variables** — Support abstract type parameters (`T`, `U`, `V`)
2. **Generic Functions** — Functions parameterized by types (e.g., `fn id<T>(x: T) -> T`)
3. **Generic Types** — Structs and enums with type parameters (e.g., `struct Box<T> { value: T }`)
4. **Type Instantiation** — Apply type arguments to generics (explicit and inferred)
5. **Monomorphization** — Specialize generic code for each concrete instantiation used
6. **Constraint Solving** — Unification algorithm to determine type arguments

## Why This Epic?

Generics enable:
- **Code reuse** — One `id` function works for all types
- **Type safety** — Generics preserve type information across abstractions
- **Standard library** — Container types like `List<T>`, `Maybe<T>`, `Result<T, E>`
- **Better error messages** — Generic constraints can be reported precisely

## Architecture

```
Type Inference Phase (from Epic 001)
  ↓
Type Variable Introduction (task 0005)
  ↓
Constraint Collection & Unification (task 0005)
  ↓
Generic Instantiation (task 0006)
  ↓
Function & Type Specialization (tasks 0007, 0008)
  ↓
Monomorphization Pass (task 0009)
  ↓
Monomorphised TypedAST (ready for evaluator)
```

## Dependencies

- **Epic 001:** Must have working type checker and basic type system
- **Parser:** Already supports generic syntax (`<T>`, generic params)
- **Types module:** Already has `Type::Named` for concrete instantiations

## Out of Scope (for Epic 002)

- Higher-ranked types (rank-N polymorphism)
- Associated types (GATs)
- Where-clauses and complex bounds
- Variance and subtyping
- Trait bounds (basic, but full trait system is separate)

## Success Criteria

When this epic is done:

- [ ] Type variables work in type inference
- [ ] Generic functions can be declared and called
- [ ] Generic functions infer type arguments correctly
- [ ] Generic structs and enums work correctly
- [ ] Explicit type arguments work (`foo::<Int>`)
- [ ] Implicit type arguments inferred from context
- [ ] Constraint solving finds type instantiations
- [ ] Monomorphization generates specialized versions
- [ ] TypedAST contains no unresolved type variables
- [ ] Evaluator executes monomorphised code correctly
- [ ] Error messages identify generic conflicts clearly
- [ ] All Epic 001 tests still pass
- [ ] Comprehensive test suite for generics

## Related Issues/Tasks

- Standard library (List<T>, Maybe<T>, Result<T, E>)
- Error recovery for generic mismatches
- Generic trait implementations (comes later)

## Notes

- Monomorphization strategy: compile-time specialization (code bloat acceptable for now)
- Type variable unification drives instantiation inference
- Generic bodies checked generically, not per instantiation
- Each instantiation gets a unique ID (`foo[Int]`, `foo[String]`, etc.)
- Recursive generics handled correctly (e.g., `List<List<Int>>`)
