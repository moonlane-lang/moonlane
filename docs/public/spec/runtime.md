# Runtime

## Panics

A panic is a hard, unrecoverable runtime error. It prints a message and exits the process with a non-zero status. Panics cannot be caught.

Panics are triggered by:
- `.yolo()` on `nope` or a `Result::Err`
- Out-of-bounds array access
- Integer division by zero
- `assert(false)` or `assert_msg(false, msg)`

## Built-in Functions

> **Temporary.** Built-in functions are a stopgap for v0.1 programs that have no module system and no standard library. Once RFC-0009 (module system) and RFC-0016 (standard library) land, these functions will migrate into stdlib modules (`std::io`, `std::string`, `std::array`, etc.) and the global built-in form will be deprecated. Do not design new language features that depend on builtins remaining as globals.

These are available globally without any `use` declaration:

| Name              | Signature                            | Description                              |
|-------------------|--------------------------------------|------------------------------------------|
| `print`           | `<T: Display>(v: T)`                 | Print to stdout, no newline              |
| `println`         | `<T: Display>(v: T)`                 | Print to stdout with newline             |
| `string_len`      | `(s: String) -> Int`                 | Number of characters in a string        |
| `string_concat`   | `(a: String, b: String) -> String`   | Concatenate two strings (also via `+`)  |
| `array_push`      | `(arr: T[], value: T)`               | Append a value (mutates the array)      |
| `array_len`       | `(arr: T[]) -> Int`                  | Number of elements in an array          |
| `clock`           | `() -> Int`                          | Unix timestamp in milliseconds          |
| `assert`          | `(cond: Bool)`                       | Panic with `"assertion failed"` if `cond` is `false` |
| `assert_msg`      | `(cond: Bool, msg: String)`          | Panic with `msg` if `cond` is `false`   |
| `dbg`             | `<T>(v: T) -> T`                     | Print `[dbg] <value>` to stderr and return the value unchanged |

**Deprecated in v0.4** (use `.to_string()` and `print`/`println` instead):

| Name              | Replacement                           |
|-------------------|---------------------------------------|
| `print_int`       | `print(n)` (polymorphic via Display)  |
| `println_int`     | `println(n)`                          |
| `print_float`     | `print(f)`                            |
| `println_float`   | `println(f)`                          |
| `int_to_string`   | `n.to_string()`                       |
| `float_to_string` | `f.to_string()`                       |
| `bool_to_string`  | `b.to_string()`                       |

## Built-in Aspects

The following aspects are pre-implemented for built-in types:

### Display

```moonlane
aspect Display {
    fun to_string(self) -> String;
}
```

`Int`, `Float`, `Bool`, and `String` implement `Display`. `.to_string()` returns the canonical string representation. `print` and `println` accept any `Display` type.

### Iterable\<T\>

```moonlane
aspect Iterable<T> {
    fun next(mut self) -> Perhaps<T>;
}
```

`T[]` (array) and `Range` (from `..` / `..=`) implement `Iterable<T>`. User-defined types may implement it to be usable in `for-in`.

### From\<S\>

```moonlane
aspect From<S> {
    fun from(value: S) -> Self;
}
```

`Int` implements `From<Float>` (truncating cast) and `Float` implements `From<Int>`. The `as` operator desugars to `T::from(value)`. User-defined types may implement `From<S>` to enable `as` casts and `?` error coercion.

## String Methods

| Method        | Signature         | Description                        |
|---------------|-------------------|------------------------------------|
| `.len()`      | `() -> Int`       | Number of characters in the string |
| `.to_string()`| `() -> String`    | Returns the string itself          |
