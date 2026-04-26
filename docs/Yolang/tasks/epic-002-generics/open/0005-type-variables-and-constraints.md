# Task 0005: Type Variables and Constraint System

**Status:** open  
**Epic:** epic-002-generics  
**Component:** typechecker  
**Spec Link:** spec/Language Spec.md#Generics  
**Blocked By:** epic-001 task 0002

## What

Implement support for **type variables** (like `T`, `U`) and a **constraint system** that drives generic type instantiation. This enables:

- Representing unknown/parameterized types during inference
- Unifying type variables with concrete types
- Detecting when a generic instantiation is required

For example, in `fn id<T>(x: T) -> T`, `T` is a type variable. When called as `id(42)`, we infer `T = Int`.

## Design

**Type Variables:**
- Add `Type::Var(String)` variant to represent type variables
- Each generic parameter becomes a type variable during inference
- Variables are scoped to their declaration (function, struct, impl block)

**Constraints:**
- Constraint: `T = ConcreteType` (a type variable must equal a type)
- Conflicts: `T = Int` and `T = Bool` (constraint unsatisfiable)
- Build constraints from unification during inference
- Solve via substitution and unification algorithm

**Unification:**
- Implement Robinson's unification algorithm or simpler variant
- Handle occurs check (prevent infinite types like `T = List<T>`)
- Return substitution map: `{T -> Int, U -> String, ...}`

## Acceptance Criteria

- [ ] `Type::Var(name)` variant added to Type enum
- [ ] Constraint structure defined (e.g., `Constraint { var: String, ty: Type }`)
- [ ] Unification function implemented and tested
- [ ] Occurs check prevents infinite types
- [ ] Constraint solver produces substitution maps
- [ ] Simple generic function inference works (e.g., `fn id<T>(x: T) -> T` inferred correctly)
- [ ] Error messages for unsatisfiable constraints
- [ ] All Epic 001 tests still pass

## Notes

- Start with monomorphic unification (each type variable unifies to exactly one concrete type)
- Constraint solving can be simple: just iterate until no changes (fixed-point)
- Don't implement ranked polymorphism or higher-rank types
- Type variable names should be human-readable (T, U, V, etc.)
