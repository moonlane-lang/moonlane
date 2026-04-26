# Task 0009: Monomorphization Engine

**Status:** open  
**Epic:** epic-002-generics  
**Component:** typechecker  
**Spec Link:** spec/Language Spec.md#Monomorphization  
**Blocked By:** 0006, 0007, 0008

## What

Implement **monomorphization** — the process of specializing generic code for concrete types at compile time.

Instead of runtime polymorphism, we generate specialized versions of generic functions/types for each concrete instantiation used in the program.

Example:
```
fn id<T>(x: T) -> T { x }

id(42)        // Needs: id<Int>(x: Int) -> Int
id("hello")   // Needs: id<String>(x: String) -> String
```

Monomorphization generates two specialized functions:
```
fn id<Int>(x: Int) -> Int { x }
fn id<String>(x: String) -> String { x }
```

The evaluator then runs the specialized, non-generic code.

## Design

**Instantiation Tracking:**
- During type checking (tasks 0006-0008), record all generic instantiations used
- Build a set: `{ (fn_id, [Int]), (fn_id, [String]), ... }`
- Also track struct/enum instantiations

**Specialization:**
1. For each recorded instantiation of a generic function:
   - Clone the function definition
   - Replace type parameters with concrete types
   - Add to a "monomorphised functions" collection
2. For each recorded instantiation of a generic struct/enum:
   - Add to a "monomorphised types" collection
   - Update the TypedAST to reference the specialized definitions

**TypedAST Update:**
- Replace generic function/type references with specialized ones
- Update function calls to use specialized versions
- Update struct/enum instantiations to use specialized definitions

**Completeness:**
- Ensure all instantiations are specializable
- Detect unused generic definitions (optimization opportunity)
- Handle recursive generics correctly

## Acceptance Criteria

- [ ] Instantiation set collected during type checking
- [ ] Generic functions monomorphised for all recorded instantiations
- [ ] Generic structs/enums monomorphised for all recorded instantiations
- [ ] TypedAST updated to reference specialized definitions
- [ ] Specialized functions have unique names/IDs
- [ ] No remaining type variables in monomorphised code
- [ ] Recursive generics handled correctly (e.g., `Box<Box<Int>>`)
- [ ] Evaluator works with monomorphised code
- [ ] Error messages clear if instantiation can't be found
- [ ] Tests cover generic functions, structs, and nested generics
- [ ] All Epic 001 tests still pass

## Notes

- Monomorphization happens at the AST level, after type checking
- Each specialization gets a unique identifier (e.g., `foo[Int]`, `foo[String]`)
- This approach (monomorphization) has code bloat risk but enables specialization optimizations
- Alternative: keep generics and use runtime polymorphism (future consideration)
- Handle circular dependencies: if `A<T>` contains `B<T>` and `B<T>` contains `A<T>`
