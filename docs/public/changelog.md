---
title: "Moonlane Language Changelog"
---

# Changelog

## v0.2

Evaluator improvements, DX features, and language quality fixes. Shipped by Sprint 3 (`sprint/3`).

**New language features:**
- Type ascription operator `:` — `[] : Int[]` guides type inference without runtime cost (RFC-0021)
- Shorthand struct field initialisation — `Point { x, y }` desugars to `Point { x: x, y: y }`
- Trailing commas allowed in function parameter lists and argument lists

**New built-in functions:**
- `assert(cond: Bool)` — panics with `"assertion failed"` if `cond` is `false`
- `assert_msg(cond: Bool, msg: String)` — panics with `msg` if `cond` is `false`
- `dbg<T>(v: T) -> T` — prints `[dbg] <value>` to stderr and returns the value unchanged
- `print_int(n: Int)`, `println_int(n: Int)` — print an `Int` without/with newline
- `print_float(f: Float)`, `println_float(f: Float)` — print a `Float` without/with newline

**Bug fixes:**
- Arrays now have value semantics — binding an array to a new variable produces an independent copy
- Error spans now report `file:line:col` instead of raw byte offsets
- Complex expressions (field access, calls) are now valid array index operands

**Developer experience:**
- Runtime panics now include a call-stack trace showing function name and call site

## v0.1

Initial language version. Implemented by the tree-walk interpreter.

**Features included:**
- Primitive types: `Int`, `Float`, `Bool`, `String`, `()`
- Variables: `let` (immutable), `mut` (mutable), lexical scoping, `fun`/type hoisting
- Functions: first-class values, closures with mutable capture, `?` operator (exact error type match only)
- Structs: literals, field access, methods (`impl`), `mut self`, associated functions
- Enums: unit and struct-like variants, `impl` blocks
- Built-in generic types: `Perhaps<T>`, `Result<T, E>`, `Array<T>` / `T[]` (as special cases; user-defined generics are v0.2)
- Exhaustive pattern matching: all pattern kinds (see [Pattern Kinds](spec/expressions.md#pattern-kinds))
- Control flow: `if`/`else`, `while`, `for`, `for-in` (arrays and ranges only), `loop`, `break`/`continue`, `return`
- Type casting: `as` for `Int ↔ Float`
- Never type (`!`)
- Tuples
- Built-in functions (see [Built-in Functions](spec/runtime.md#built-in-functions))

**Not included (v0.2+):**
- User-defined generic functions and types (see [Generics](spec/types.md#generics))
- User-defined traits and `impl Trait for Type` (see [Traits](spec/declarations.md#traits))
- `From`-based `?` coercion across different error types (see [The ? Operator](spec/functions.md#the--operator))
- User-defined `Iterable<T>` implementations (see [For-In](spec/expressions.md#for-in))
