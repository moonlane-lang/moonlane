---
title: "Moonlane Language Changelog"
---

# Changelog

## v0.5.0

Module system. Shipped by Sprint 9 (`sprint/9`).

**New language features:**
- Multi-file programs: each `.mln` file is a module; the module graph is built from `import` declarations
- `import path::Name;` both loads the referenced file and brings `Name` into scope
- Import forms: single name, alias (`as`), group (`{A, B}`), glob (`*`), module handle
- `export path::Name;` re-exports a name from a submodule into the current module's public API
- `pub` on `fun`, `struct`, `enum`, and `aspect` marks declarations as externally accessible
- Absolute and relative path roots: `root::`, `std::`, `self::`, `super::`
- Fully-qualified paths valid in type and expression position without a preceding `import`
- Circular imports detected at load time with a full chain in the error message
- Facade modules: `parser.mln` alongside `parser/` directory — no special `mod.mln` file
- File-to-module mapping via `::` → `/` with no special cases

**Deferred to a future release:**
- `std::core` auto-import and standard library core types (#150)

**Compatibility:**
- Single-file programs with no `import` or `export` declarations remain valid without modification

## v0.4.2

Evaluator refactor, test restructure, and keyword cleanup. Shipped by Sprint 8 (`sprint/8`).

**Breaking changes:**
- `Perhaps::Nope` renamed to `Perhaps::None`; the standalone `nope` keyword is now `None`

## v0.4.1

Technical debt, bug fixes, and internal cleanup. Shipped by Sprint 7 (`sprint/7`).

**Bug fixes:**
- `TypeErrorCode::T0005` ("Invalid operand types") is now emitted for arithmetic operators (`+`, `-`, `*`, `/`, `%`) applied to non-numeric types (e.g. `true + false` is now a type error)
- Unary negation (`-`) on non-numeric types is now a type error
- Ordering comparisons (`<`, `<=`, `>`, `>=`) on non-comparable types (non-Int, non-Float, non-String) are now type errors
- `Pattern::Nope` latent bug eliminated — `nope` values are now exclusively `Value::Perhaps(None)`, so the pattern can no longer silently miss the `Value::Enum { name: "Perhaps", variant: "Nope" }` form

**Internal improvements:**
- `Value::YoloResult` renamed to `Value::Result`; `Perhaps` and `Result` values are now first-class runtime variants — no longer stored as `Value::Enum`
- Large enum variants boxed in `Decl`, `Stmt`, `TypedDecl`, `TypedStmt` (stack frame sizes reduced from 896–1040 bytes to 8 bytes)
- Dead utility methods removed (`Program::new`, `Type::is_numeric`, `Type::is_unit`); reserved fields annotated with `#[allow(dead_code)]`
- All clippy style/idiom warnings resolved

## v0.4.0

Aspects and upgraded builtins. Shipped by Sprint 6 (`sprint/6`).

**New language features:**
- Aspect declarations — `aspect Foo { fun method(self) -> T; }`
- `impl Aspect for Type` blocks with method dispatch via `.method()` syntax
- `Iterable<T>` aspect — user-defined types usable in `for-in` loops
- `From<S>` aspect — `as` cast desugars to `T::from(value)`; user-defined casts for any type pair
- `Display` aspect — `.to_string()` on `Int`, `Float`, `Bool`, `String`; `print`/`println` polymorphic via Display
- `?` operator now supports cross-type error coercion: if the function's error type `E2` implements `From<E1>`, `?` calls `E2::from(e)` automatically

**Builtin changes:**
- `print(v)` and `println(v)` are now polymorphic (`<T: Display>`) — accept any Display type
- `Int::from(f: Float)` and `Float::from(n: Int)` built-in From impls replace the hardcoded `as` special case
- Deprecated: `print_int`, `println_int`, `print_float`, `println_float`, `int_to_string`, `float_to_string`, `bool_to_string` (use `.to_string()` and polymorphic `print`/`println`)

**Bug fixes:**
- Keyword-prefix identifiers (`break_sum`, `return_value`, `let_x`) now parse correctly as identifiers
- Multiple `impl From<X> for Y` blocks with different source types now dispatch independently

## v0.3.0

Generics and type-inference improvements. Shipped by Sprint 5 (`sprint/5`).

**New language features:**
- User-defined generic functions — `fun id<T>(x: T) -> T` — monomorphised at each call site
- User-defined generic structs — `struct Box<T> { value: T }`, `struct Pair<A, B> { ... }`
- User-defined generic enums — `enum Maybe<T> { Some { value: T }, None {} }`
- Let-polymorphism — unannotated `let`-bound closures are generalised to polymorphic schemes (`let id = fun(x) { x }` works at `Int`, `Bool`, and `String` in the same scope)
- Braceless `if` body — `if (c) expr` and `if (c) a else b` (RFC-0022)
- `struct` and `enum` declarations are allowed inside function bodies

**Type-inference improvements:**
- `expected_ty` propagates into match arm bodies — bare `[]` and `nope` resolve without ascription when the surrounding return type is known
- Callee parameter types propagate into argument construction — `find(words, nope)` resolves without ascription when the parameter type is `Perhaps<String>`
- Lvalue path assignment — `obj.field = val` and `arr[i] = val` work on non-bare receivers (e.g. `get_foo().bar = 1`)

## v0.2.0

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

## v0.1.0

Initial language version. Implemented by the tree-walk interpreter.

**Features included:**
- Primitive types: `Int`, `Float`, `Bool`, `String`, `()`
- Variables: `let` (immutable), `mut` (mutable), lexical scoping, `fun`/type hoisting
- Functions: first-class values, closures with mutable capture, `?` operator (exact error type match only)
- Structs: literals, field access, methods (`impl`), `mut self`, associated functions
- Enums: unit and struct-like variants, `impl` blocks
- Built-in generic types: `Perhaps<T>`, `Result<T, E>`, `Array<T>` / `T[]` (as special cases; user-defined generics are v0.3.0)
- Exhaustive pattern matching: all pattern kinds (see [Pattern Kinds](spec/expressions.md#pattern-kinds))
- Control flow: `if`/`else`, `while`, `for`, `for-in` (arrays and ranges only), `loop`, `break`/`continue`, `return`
- Type casting: `as` for `Int ↔ Float`
- Never type (`!`)
- Tuples
- Built-in functions (see [Built-in Functions](spec/runtime.md#built-in-functions))

**Not included (v0.3.0+):**
- User-defined generic functions and types (see [Generics](spec/types.md#generics))
- User-defined aspects and `impl Aspect for Type` (see [Aspects](spec/declarations.md#aspects))
- `From`-based `?` coercion across different error types (see [The ? Operator](spec/functions.md#the--operator))
- User-defined `Iterable<T>` implementations (see [For-In](spec/expressions.md#for-in))
