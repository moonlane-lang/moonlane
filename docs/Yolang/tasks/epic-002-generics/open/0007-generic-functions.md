# Task 0007: Generic Function Type Checking

**Status:** open  
**Epic:** epic-002-generics  
**Component:** typechecker  
**Spec Link:** spec/Language Spec.md#Generic-Functions  
**Blocked By:** 0005, 0006

## What

Enable **generic functions** — functions with type parameters that work over multiple types.

Examples:
```
fn id<T>(x: T) -> T { x }
fn map<T, U>(xs: Array<T>, f: fn(T) -> U) -> Array<U> { ... }
fn swap<A, B>(x: A, y: B) -> (B, A) { (y, x) }
```

This task handles:
1. **Generic parameter declaration:** `<T>`, `<T, U, V>`
2. **Using type parameters in signatures:** Parameters, return types, field types
3. **Type checking function bodies:** With type parameters as abstract types
4. **Instantiation at call sites:** Inferring or providing explicit type arguments

## Design

**Generic Function Representation:**
- Store function signature with type parameters
- During type checking, treat type parameters as type variables
- When called, instantiate the signature with concrete types

**Type Checking Generic Bodies:**
- Substitute type variables in the function body
- Type check body with substituted types
- Constraints from body propagate to type parameter usage

**Function Calls:**
- At call site: infer type arguments from argument types
- Match argument types to parameter types (with type variables)
- Unify to determine type arguments

## Acceptance Criteria

- [ ] Generic function declarations parse and type-check
- [ ] Type parameters can be used in parameter types and return type
- [ ] Generic function bodies type-check correctly
- [ ] Generic functions can be called with explicit type arguments (`foo::<Int>`)
- [ ] Generic functions can be called with inferred type arguments (`foo(42)`)
- [ ] Type inference correctly deduces type arguments from call arguments
- [ ] Nested generic functions work (generic function returns another generic function)
- [ ] Higher-order functions work (taking functions as parameters)
- [ ] Error messages identify which type parameter caused a mismatch
- [ ] Tests cover common patterns (id, map, fold, etc.)
- [ ] All Epic 001 tests still pass

## Notes

- Focus on monomorphic instantiation (each type param maps to exactly one type)
- Don't implement implicit higher-rank polymorphism
- Generic bodies should be checked generically (not per instantiation at this stage)
- Actual specialization happens in task 0009 (monomorphization)
