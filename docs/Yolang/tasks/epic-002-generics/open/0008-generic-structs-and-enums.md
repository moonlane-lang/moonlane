# Task 0008: Generic Struct and Enum Type Checking

**Status:** open  
**Epic:** epic-002-generics  
**Component:** typechecker  
**Spec Link:** spec/Language Spec.md#Generic-Structs  
**Blocked By:** 0005, 0006

## What

Enable **generic structs and enums** — aggregate types with type parameters.

Examples:
```
struct Box<T> {
    value: T
}

struct Pair<A, B> {
    first: A,
    second: B
}

enum Maybe<T> {
    Some(T),
    None
}
```

This task handles:
1. **Generic struct/enum declarations:** With type parameters
2. **Field types using parameters:** `value: T` where `T` is a parameter
3. **Construction:** `Box { value: 42 }` with inferred type
4. **Construction with explicit types:** `Box::<Int> { value: 42 }`
5. **Field access:** Type-checking against the instantiated type

## Design

**Generic Type Declaration:**
- Store struct/enum definition with type parameters
- During type checking, treat type parameters as type variables

**Instantiation:**
- At construction: determine type arguments (inferred or explicit)
- Substitute parameters in field types
- Validate field assignments against substituted types

**Field Access:**
- Track the instantiated type of a struct value
- Field access uses the instantiated type, not the generic definition

**Built-in Generics:**
- `Array<T>` — built-in generic array (basic support from Epic 001, full generics here)
- `Maybe<T>` or similar — built-in optional type
- These are special-cased in the type system

## Acceptance Criteria

- [ ] Generic struct declarations parse and type-check
- [ ] Generic enum declarations parse and type-check
- [ ] Type parameters can be used in field types
- [ ] Struct construction works with explicit type arguments
- [ ] Struct construction works with inferred type arguments
- [ ] Enum variant construction works with generic types
- [ ] Field access returns the correct instantiated type
- [ ] Pattern matching works on generic enum variants
- [ ] Nested generics work (`Box<Pair<Int, String>>`)
- [ ] Error messages identify type parameter mismatches
- [ ] Tests cover common patterns (Box, Pair, Maybe, etc.)
- [ ] All Epic 001 tests still pass

## Notes

- Focus on simple generics; don't implement GATs (generic associated types) yet
- Built-in types like `Array<T>` should work seamlessly
- Pattern matching on generic types needs special handling
- Struct/enum types are named (e.g., `Box<Int>` is a `Named("Box", [Int])`)
