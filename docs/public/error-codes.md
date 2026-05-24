# Moonlane Error Code Reference

All Moonlane errors carry a code. Codes are prefixed by phase:

| Prefix | Phase |
|---|---|
| `P` | Parse — invalid source text |
| `T` | Type — type-checker rejection |
| `R` | Runtime — error during execution |
| `I` | Internal — bug in the interpreter (please report) |

---

## Parse errors (P)

### P0001 — Syntax error

The source text does not match the Moonlane grammar.

```
[P0001] parse error in main.mln at 12..18 (`let x = ;`): expected expression
```

**Fix:** correct the syntax at the indicated position.

### P0002 — Invalid integer literal

An integer literal is out of range for `i64` (−9,223,372,036,854,775,808 to 9,223,372,036,854,775,807).

```
[P0002] parse error in main.mln at 4..24: integer literal '99999999999999999999' is out of range for i64
```

**Fix:** use a value that fits in `i64`, or split the computation.

### P0003 — Invalid float literal

A float literal cannot be represented as an `f64`.

```
[P0003] parse error in main.mln at 4..12: invalid float literal '1e9999'
```

**Fix:** use a value within the `f64` range (~±1.8 × 10³⁰⁸).

---

## Type errors (T)

### T0001 — Type mismatch

Two types that must be equal are not.

```
[T0001] type error in main.mln at 10..20: expected Int, got Bool
```

**Fix:** ensure the expression produces the expected type. Add an explicit cast if widening (e.g. `x as Float`).

### T0002 — Annotation required

The type checker cannot infer a type without an explicit annotation.

```
[T0002] type error in main.mln at 5..10: cannot infer type of `x`; add a type annotation
```

**Fix:** annotate the binding: `let x: Int = ...`.

### T0003 — Undefined name

A name is used but not defined in the current scope.

```
[T0003] type error in main.mln at 8..12: undefined name `foo`
```

**Fix:** define the variable or function before use, or correct the spelling.

### T0004 — Arity mismatch

A function is called with the wrong number of arguments.

```
[T0004] type error in main.mln at 5..20: expected 2 arguments, got 3
```

**Fix:** pass the exact number of arguments the function declares.

### T0005 — Invalid operand types

A binary operator is applied to types it does not support.

```
[T0005] type error in main.mln at 6..13: operator `+` cannot be applied to Bool and Int
```

**Fix:** use compatible types, or cast one operand.

### T0006 — Assignment to immutable binding

A `let` binding is assigned after initial definition.

```
[T0006] type error in main.mln at 3..12: `x` is immutable; use `mut x` to allow reassignment
```

**Fix:** change the binding declaration to `mut`.

### T0007 — Invalid cast

A `as` cast between incompatible types.

```
[T0007] type error in main.mln at 5..15: cannot cast Bool to Int
```

**Fix:** only cast between numeric types (`Int as Float`). Use an explicit conversion function for other types.

### T0008 — Non-exhaustive match

A `match` expression does not cover all possible values of the scrutinee type.

```
[T0008] type error in main.mln at 2..30: match on Colour is non-exhaustive; missing variant `Blue`
```

**Fix:** add the missing arms, or add a wildcard arm `_ => ...`.

---

## Runtime errors (R)

### R0001 — No `main` function defined

Execution requires a `main` function but none was found.

```
[R0001] runtime error in main.mln at 0..0: no main() function defined
```

**Fix:** add `fn main() { ... }` to your program.

### R0002 — `main` is not a valid entry point

`main` exists but is generic or is not a function.

```
[R0002] runtime error in main.mln at 0..0: main() is generic — not supported in v0.1
```

**Fix:** `main` must be a concrete, non-generic function with no parameters.

### R0003 — Undefined variable at runtime

A variable name is not found in the current environment. This can occur when a variable is used before it is defined in a branch that the type-checker did not flag.

```
[R0003] runtime error in main.mln at 10..15: undefined variable `x`
```

### R0004 — Index out of bounds

An array index is negative or ≥ the array length.

```
[R0004] runtime error in main.mln at 5..10: index 5 out of bounds (len 3)
```

**Fix:** check that the index is within `0..array.len()` before access.

### R0005 — Tuple index out of bounds

A tuple element is accessed by an index that does not exist.

```
[R0005] runtime error in main.mln at 5..10: tuple index 3 out of bounds
```

**Fix:** tuple indices are fixed at compile time; verify the index against the tuple's declared length.

### R0006 — Non-exhaustive match at runtime

A `match` expression reached its end without any arm matching. This indicates a pattern that the type checker approved as exhaustive but that is not, which is a known limitation.

```
[R0006] runtime error in main.mln at 2..30: match: no arm matched scrutinee
```

### R0007 — Arithmetic error

Integer division or remainder by zero.

```
[R0007] runtime error in main.mln at 8..13: division by zero
```

**Fix:** guard with a zero check before dividing.

### R0008 — Field not found

A struct or enum value does not have the accessed field.

```
[R0008] runtime error in main.mln at 5..12: no field `colour` on value
```

**Fix:** check the field name against the type definition.

### R0009 — Method not found

A method call cannot be resolved for the receiver type.

```
[R0009] runtime error in main.mln at 5..20: no method `draw` on `Circle`
```

**Fix:** define the method in an `impl` block for the type.

### R0010 — Call on non-callable value

A call expression (`f(...)`) is applied to a value that is not a function or closure.

```
[R0010] runtime error in main.mln at 3..8: call: expected a closure or builtin
```

### R0011 — Invalid for-in iterator

A `for x in expr` loop where `expr` does not evaluate to an `Array` or `Range`.

```
[R0011] runtime error in main.mln at 1..20: for-in: expected Array or Range
```

**Fix:** ensure the iterable is an array literal, a range (`a..b`), or a variable of those types.

### R0012 — Error propagation on non-Result value

The `?` operator is applied to a value that is not a `Result`.

```
[R0012] runtime error in main.mln at 5..10: ?: expected a Result value
```

**Fix:** only use `?` on expressions whose type is `Result[T, E]`.

---

## Internal errors (I)

### I0001 — Internal interpreter error

The interpreter reached an impossible state. This is a bug in the interpreter — the typechecker should have caught it before execution.

```
[I0001] internal error: binop: unsupported operand types (typechecker should have caught this)
```

**What to do:** please file a bug report at <https://github.com/Vladastos/moonlane/issues> with the source program that triggered this error.

### I0002 — Not implemented

The program uses a language feature that is not yet supported in this version of the interpreter.

```
[I0002] internal error: generic functions are not supported in v0.1
```

**What to do:** check the [changelog](changelog.md) for the current supported feature set and the [versioning guide](../internal/versioning.md) for the planned implementation milestone.
