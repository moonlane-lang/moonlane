# Epic 001: Typechecker and Typed AST

**Status:** open  
**Started:** 2026-04-26

## Overview

Build a complete type-checking system with an AST that carries type information throughout evaluation. This epic establishes the foundation for type safety, enables better error reporting, and separates compile-time checking from runtime execution.

## Goals

1. **Typed AST Representation** — Define AST nodes that carry type annotations from parsing through evaluation
2. **Type Inference Engine** — Infer types where not explicitly annotated (following Hindley-Milner or similar)
3. **Type Checker** — Validate programs before execution; catch type errors early
4. **Basic Type Support** — Implement int, bool, string, and array types with proper coercion rules
5. **Clear Type Error Messages** — Report type mismatches with context and suggestions

## Why This Epic?

**Type safety** is foundational. Right now the interpreter is dynamically typed, which works but:
- Errors appear at runtime, sometimes far from the cause
- No IDE support (no type information to work with)
- Performance can't be optimized without type info
- Error messages are vague ("can't add these things")

A typed AST enables:
- Compile-time error checking
- Better IDE integration in the future
- Foundation for optimization
- Clear, actionable error messages

## Architecture

```
Source Code
    ↓
Parser → **Typed AST** ← Type Inference + Checking
    ↓
Evaluator (runs typed AST)
    ↓
Output
```

The **Typed AST** is the centerpiece: parser builds it with initial types, type checker refines and validates, evaluator runs it with all type info available.

## Dependencies

- **Parser:** Must support type annotations (e.g., `x: int = 42`)
- **Spec:** Language spec must define type system (basic types, coercion, subtyping rules)
- **Error Handling:** Type errors need good error recovery

## Out of Scope (for Epic 001)

- **Generics and parametric polymorphism** (Epic 002)
- **Monomorphization** (Epic 002)
- **List<T>** — only base Array type for now
- Type aliases or custom type definitions
- Trait/interface systems
- Variance and subtyping complexity

## Success Criteria

When this epic is done:

- [ ] Parser accepts and preserves type annotations
- [ ] AST nodes carry type information
- [ ] Type inference passes all test cases
- [ ] Type checker catches invalid type operations
- [ ] Basic types work: Int, Float, Bool, String, Array, Unit, Tuple
- [ ] Array operations type-check correctly
- [ ] Evaluator reads type info and validates operations
- [ ] Type error messages are clear and actionable
- [ ] No regressions in existing parser/evaluator tests
- [ ] Spec has a "Type System" section documenting types, coercion, and rules

## Related Issues/Tasks

- Language Spec (needs type system section)
- Error recovery in parser
- REPL improvements (will benefit from type info)

## Notes

- Use simple, concrete type inference (no type variables yet)
- Keep type checking separate from evaluation for clarity
- Build test suite incrementally: type checking tests before implementation
- Focus on correctness; generics and advanced features are Epic 002
- Array is homogeneous: `Array<Int>` at runtime, but represented as single-element-type
