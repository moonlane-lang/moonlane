# Task 0006: Generic Type Instantiation

**Status:** open  
**Epic:** epic-002-generics  
**Component:** typechecker  
**Spec Link:** spec/Language Spec.md#Generics  
**Blocked By:** 0005

## What

Implement **type instantiation** — the ability to apply type arguments to generic types and detect when instantiation is needed.

For example:
- `Box<Int>` — applying `Int` to the type parameter `T` of `Box<T>`
- `fn foo<T>(x: T)` called with `foo(42)` — instantiate `T = Int`

This task handles:
1. **Explicit instantiation:** `Box<Int>`, `foo::<Int>(...)`
2. **Implicit instantiation:** Inferring type arguments from call sites
3. **Instantiation tracking:** Recording which instantiations are used (for monomorphization)

## Design

**Instantiation Representation:**
- Generic definitions (structs, functions) have type parameters (e.g., `<T>`, `<T, U>`)
- Instantiations substitute type variables with concrete types (e.g., `{T -> Int}`)
- Record all instantiations needed during type checking

**Implicit Instantiation (Inference):**
- When a generic function is called without explicit type args, infer them
- Example: `id(42)` → infer `T = Int`
- Example: `map<T>(xs: Array<T>, f: fn(T) -> U) -> Array<U>` called with integers → infer `T = Int`

**Type Argument Validation:**
- Check that type arguments satisfy any bounds (future: where-clauses)
- For now, any concrete type is valid

## Acceptance Criteria

- [ ] Explicit type instantiation works (`Box<Int>`, `foo::<Bool>`)
- [ ] Implicit instantiation inferred from call arguments
- [ ] Type variables in function parameters matched against call arguments
- [ ] Multiple type parameters instantiated correctly
- [ ] Instantiation substitution applied correctly to function bodies
- [ ] Instantiation decisions recorded for monomorphization (task 0009)
- [ ] Error messages for mismatched instantiations
- [ ] Tests cover generic functions and structs
- [ ] All Epic 001 tests still pass

## Notes

- Implicit instantiation requires bidirectional type checking (checking mode + inference mode)
- Keep instantiation decisions in a global map: `Map<(FnName, [TypeArgs]), Instantiation>`
- This sets up for monomorphization in task 0009
- Don't implement rank-N types or higher-rank polymorphism
