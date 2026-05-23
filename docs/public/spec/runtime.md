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

| Name              | Signature                           | Description                              |
|-------------------|-------------------------------------|------------------------------------------|
| `print`           | `(s: String)`                       | Print to stdout, no newline              |
| `println`         | `(s: String)`                       | Print to stdout with newline             |
| `print_int`       | `(n: Int)`                          | Print an Int to stdout, no newline       |
| `println_int`     | `(n: Int)`                          | Print an Int to stdout with newline      |
| `print_float`     | `(f: Float)`                        | Print a Float to stdout, no newline      |
| `println_float`   | `(f: Float)`                        | Print a Float to stdout with newline     |
| `int_to_string`   | `(n: Int) -> String`                | Decimal string representation of an Int |
| `float_to_string` | `(n: Float) -> String`              | String representation of a Float        |
| `bool_to_string`  | `(b: Bool) -> String`               | `"true"` or `"false"`                   |
| `string_len`      | `(s: String) -> Int`                | Number of characters in a string        |
| `string_concat`   | `(a: String, b: String) -> String`  | Concatenate two strings (also via `+`)  |
| `array_push`      | `(arr: T[], value: T)`              | Append a value (mutates the array)      |
| `array_len`       | `(arr: T[]) -> Int`                 | Number of elements in an array          |
| `clock`           | `() -> Int`                         | Unix timestamp in milliseconds          |
| `assert`          | `(cond: Bool)`                      | Panic with `"assertion failed"` if `cond` is `false` |
| `assert_msg`      | `(cond: Bool, msg: String)`         | Panic with `msg` if `cond` is `false`   |
| `dbg`             | `<T>(v: T) -> T`                    | Print `[dbg] <value>` to stderr and return the value unchanged |
