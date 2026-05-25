# Functions

```moonlane
fun add(a: Int, b: Int) -> Int {
    return a + b;
}
```

Parameter type annotations are optional when types can be inferred from context. The return type follows `->` and is also optional — a function with no return annotation and no `return expr;` returns `()`. `return expr;` and bare `return;` are both valid.

## Associated Functions

`impl` blocks may contain functions with no `self` parameter. These are called on the type via `::` syntax and serve as the canonical constructor pattern:

```moonlane
impl Point {
    fun new(x: Float, y: Float) -> Point {
        return Point { x: x, y: y };
    }
}

let p = Point::new(1.0, 2.0);
```

## First-Class Functions

Functions are first-class values and can be assigned, passed, and returned:

```moonlane
let f = add;
f(1, 2);   // 3

fun apply(f: fun(Int) -> Int, x: Int) -> Int {
    return f(x);
}
```

The type of a function or closure is written as `fun(ParamTypes) -> ReturnType`.

## Closures

Anonymous functions are written with `fun` in expression position:

```moonlane
let double = fun(x: Int) -> Int { return x * 2; };
double(5);   // 10
```

Closures capture variables from their enclosing scope. Captured `mut` variables are shared — mutations are visible in the outer scope:

```moonlane
mut count = 0;
let inc = fun() { count += 1; };
inc();
inc();
// count == 2
```

## The ? Operator

Inside a function returning `Result<T, E>`, `?` propagates errors early:

```moonlane
fun parse_and_double(s: String) -> Result<Int, String> {
    let n = parse_int(s)?;   // returns Err early if parse_int fails
    return Result::Ok { value: n * 2 };
}
```

`?` desugars to: if the expression is `Err(e)`, return `Err(E2::from(e))` immediately (where `E2` is the enclosing function's error type); otherwise unwrap to the `Ok` value.

The inner expression's error type `E1` and the function's return error type `E2` must satisfy `E2: From<E1>`. When `E1 == E2` no conversion is performed. When they differ, `From::from` is called automatically on the error value before re-wrapping in `Err`.

> **v0.1–v0.3:** Error types must match exactly (`E1 == E2`). `From`-based coercion between different error types is a v0.4 feature.
