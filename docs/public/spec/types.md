# Type System

Moonlane is statically and strongly typed. Types are checked at compile time. There are no implicit conversions.

## Primitive Types

| Type     | Description               | Example   |
|----------|---------------------------|-----------|
| `Int`    | 64-bit signed integer     | `42`      |
| `Float`  | 64-bit floating point     | `3.14`    |
| `Bool`   | Boolean                   | `true`    |
| `String` | UTF-8 string              | `"hello"` |
| `()`     | Unit — represents no value | `()`     |

The unit type `()` is only written explicitly when needed as a type parameter (e.g. `Result<(), Error>`). Functions that return nothing omit the `->` annotation entirely.

## Type Inference

Types are inferred using the Hindley-Milner algorithm with let-polymorphism. Annotations are optional for all bindings, including function parameters and return types. They may be written explicitly for documentation or to restrict a binding to a less general type.

Annotations are required only where there is no expression to infer from:
- Struct and enum field types
- Aspect method signatures

```moonlane
let x = 42;           // inferred: Int
let name = "Vlad";    // inferred: String
let y: Float = 3.14;  // explicit annotation (optional here)

fun add(a: Int, b: Int) -> Int { a + b }   // annotated
fun add(a, b) { a + b }                    // also valid; inferred from use
```

## Tuples

Tuples are lightweight anonymous product types.

```moonlane
let coord: (Int, Int) = (10, 20);
let triple: (String, Int, Bool) = ("yes", 42, true);
```

Positional field access uses `.0`, `.1`, etc.:

```moonlane
let x = coord.0;   // 10
let y = coord.1;   // 20
```

`()` is the zero-element tuple (unit type).

Tuples can be destructured in `match`:

```moonlane
match coord {
    (0, y) => println("on y-axis"),
    (x, 0) => println("on x-axis"),
    (x, y) => println("elsewhere"),
}
```

## Arrays

`Array<T>` is the built-in ordered sequence type. The shorthand `T[]` is preferred.

```moonlane
let nums: Int[] = [1, 2, 3];
let names: Array<String> = ["alice", "bob"];
```

Index access uses `[]` with an `Int` index. Out-of-bounds access causes a panic.

```moonlane
let first = nums[0];
```

Arrays are usable in `for-in` loops. `List<T>` is not available in v0.1; `T[]` is the only sequence type.

## Type Ascription

The `:` operator asserts that an expression has a given type without performing any runtime conversion. It is a pure type-inference hint — no code is emitted at runtime.

```moonlane
let xs = [] : Int[];        // element type resolved to Int
let x  = 1 : Int;          // identity ascription — no effect at runtime
let y  = 1 : String;       // compile error — Int is not String
```

Ascription fails at compile time if the inferred type of the sub-expression cannot be unified with the ascribed type. Use `as` to convert between types; use `:` only when the value already has the target type.

### When ascription is needed

Type inference flows from the outside in via `let` annotations and function return types. It does not flow from a function's parameter types back into its arguments (see [#115](https://github.com/moonlane-lang/moonlane/issues/115)), and it does not flow from a match expression's expected type into arm bodies (see [#114](https://github.com/moonlane-lang/moonlane/issues/114)). In those positions there is no binding to annotate, so ascription is the only inline option.

**Argument position — empty array**

The function's parameter type is not used as `expected_ty` for the argument expression. A bare `[]` at a call site has no type context and fails.

```moonlane
fun process(items: Int[]) { ... }

process([]);             // error — element type of [] cannot be inferred
process([] : Int[]);     // ok

// Alternative: hoist a binding — correct, but adds a throwaway name.
let items: Int[] = [];
process(items);
```

**Argument position — `nope`**

`nope` has no fields, so its type parameter cannot be resolved from the value alone.

```moonlane
fun find(haystack: String[], fallback: Perhaps<String>) -> String { ... }

find(words, nope);                       // error — type of nope cannot be inferred
find(words, nope : Perhaps<String>);     // ok

// Alternative: hoist a binding.
let nothing: Perhaps<String> = nope;
find(words, nothing);
```

**Match arm body**

A match arm body is an expression context — `let` is not syntactically valid there without a block wrapper. The expected type of the match expression does not currently flow into arm bodies, so ambiguous literals in arms must be ascribed.

```moonlane
fun default_row(use_default: Bool, fallback: Int[]) -> Int[] {
    match use_default {
        true  => [],          // error — element type of [] cannot be inferred
        false => fallback,
    }
}

fun default_row(use_default: Bool, fallback: Int[]) -> Int[] {
    match use_default {
        true  => [] : Int[],  // ok — ascription supplies the missing context
        false => fallback,
    }
}

// Alternative: block arm with a local binding — correct, but noisier.
fun default_row(use_default: Bool, fallback: Int[]) -> Int[] {
    match use_default {
        true  => { let empty: Int[] = []; empty },
        false => fallback,
    }
}
```

**Two ambiguous arguments**

When both arguments are empty literals with different element types, neither anchors the other, and two bindings would be needed.

```moonlane
fun zip_lengths(a: Int[], b: String[]) -> Int { ... }

zip_lengths([], []);                          // error — both [] are ambiguous
zip_lengths([] : Int[], [] : String[]);       // ok
```

## Type Casting

The `as` operator casts between numeric primitive types. It desugars to a call to the `From` aspect and is infallible — the result is the target type directly.

```moonlane
let x: Int = 42;
let f: Float = x as Float;

let f2: Float = 3.99;
let i: Int = f2 as Int;   // truncates toward zero
```

Allowed primitive casts: `Int` ↔ `Float`.

Because `as` desugars to `From`, user-defined types become castable by implementing `From<SourceType>` for the target type.

## Generics

> **v0.3 feature.** User-defined generic functions and types are available from v0.3.
> Built-in generic types (`Perhaps<T>`, `Result<T, E>`, `T[]`) were supported as
> special cases from v0.1.

Types and functions can be parameterized with `<T>` syntax.

```moonlane
struct Stack<T> {
    items: T[],
}

fun first<T>(arr: T[]) -> Perhaps<T> { ... }
```

Constraints are expressed with `where` clauses or inline bounds:

```moonlane
fun largest<T>(a: T, b: T) -> T where T: Comparable { ... }

fun largest<T: Comparable>(a: T, b: T) -> T { ... }  // inline form
```

## Never Type

`!` (Never) is the bottom type — the type of an expression that never produces a value because it diverges (runs forever, panics, or exits). A `loop` with no reachable `break` has type `!`:

```moonlane
let x: ! = loop { };         // runs forever — type is !
let y: ! = loop { panic!(); };
```

Because `!` coerces to every type, it can appear where any type is expected:

```moonlane
let result: Int = loop { break 42; };  // break gives the loop type Int
let diverge: Int = loop { };           // ! coerces to Int — dead code after
```

`!` is not a type users write in practice; it appears as an inferred type when the typechecker determines a branch or expression cannot return. It is the type of `return` and `panic!` expressions as well.

## Perhaps<T>

`Perhaps<T>` is the built-in optional type. There is no null — all absence is expressed via `Perhaps<T>`.

The type of `nope` is `Perhaps<T>` for some `T` that must be determinable from context. If no context constrains `T` — for example, a bare `let x = nope` with no annotation and no subsequent use that pins the element type — the program is a type error. An explicit annotation is required in that case:

```moonlane
let x = nope;              // ERROR: cannot infer type of `nope`
let x: Perhaps<Int> = nope; // OK
```

```moonlane
let result: Perhaps<Int> = nope;
let value: Perhaps<Int> = 42;
```

Use `match` to unwrap safely:

```moonlane
match find_user(1) {
    Perhaps::Some { value } => println(value.name),
    Perhaps::Nope => println("not found"),
}
```

`.yolo()` unwraps, panicking if the value is `nope`:

```moonlane
let user = find_user(1).yolo();
```

## Result<T, E>

`Result<T, E>` represents the outcome of a fallible operation:

```moonlane
fun divide(a: Float, b: Float) -> Result<Float, String> {
    if (b == 0.0) {
        return Result::Err { error: "division by zero" };
    }
    return Result::Ok { value: a / b };
}
```

Use `match` to handle both cases, or `?` to propagate errors.

`.yolo()` also works on `Result<T, E>`, panicking on `Err`.
