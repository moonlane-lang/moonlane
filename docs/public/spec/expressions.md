# Expressions

## Pattern Matching

`match` performs exhaustive pattern matching. All cases must be covered.

```moonlane
match value {
    pattern => expression,
    _       => expression,   // catch-all
}
```

Each arm body can be a single expression **or** a block:

```moonlane
match value {
    pattern => expression,
    _       => { stmts* expression? },
}
```

`match` is an expression — all arms must produce the same type:

```moonlane
let label = match x {
    0 => "zero",
    1 => "one",
    _ => "other",
};
```

Arms with blocks follow the same rules as function bodies: the block's tail expression (if present) is the arm's value; a block with no tail produces `Unit`.

```moonlane
let desc: String = match shape {
    Shape::Circle { radius } => {
        let area = radius * radius;
        int_to_string(area as Int)
    },
    Shape::Rectangle { width, height } => "rectangle",
};
```

### Pattern Kinds

| Pattern | Example | Matches |
|---------|---------|---------|
| Wildcard | `_` | anything, binds nothing |
| Binding | `n` | anything, binds to `n` |
| Literal | `0`, `"hi"`, `true`, `nope` | exact value |
| Enum variant | `Direction::North` | unit variant |
| Enum with fields | `Shape::Circle { radius }` | variant, binds fields |
| Tuple | `(a, b)` | tuple, binds elements |
| Guard | `n if n < 0` | binding + boolean condition |

### Examples

```moonlane
// enum destructuring
match shape {
    Shape::Circle { radius } => println(float_to_string(radius)),
    Shape::Rectangle { width, height } => println(float_to_string(width)),
}

// literal and guard
match x {
    0           => println("zero"),
    n if n < 0  => println("negative"),
    _           => println("positive"),
}

// tuple destructuring
match point {
    (0, 0) => println("origin"),
    (x, 0) => println("on x-axis"),
    (0, y) => println("on y-axis"),
    (x, y) => println("elsewhere"),
}
```

---

## Control Flow

### If / Else

```moonlane
if (condition) {
    // ...
} else if (other) {
    // ...
} else {
    // ...
}
```

`if` is also an expression (both branches must produce the same type):

```moonlane
let label = if (x > 0) { "positive" } else { "non-positive" };
```

### While

```moonlane
while (condition) {
    // ...
}
```

### For

```moonlane
for (mut i = 0; i < 10; i += 1) {
    // ...
}
```

### For-In

`for-in` works on any type implementing the `Iterable<T>` trait. The loop variable
receives type `T`. `T[]` (array) and `Range` (produced by `..` and `..=`) implement
`Iterable<T>` by default. User-defined types can be made iterable by implementing
`Iterable<T>`:

```moonlane
trait Iterable<T> {
    fun next(mut self) -> Perhaps<T>;
}
```

```moonlane
for (let item in collection) { ... }
for (let i in 0..10) { ... }    // 0, 1, ..., 9
for (let i in 0..=10) { ... }   // 0, 1, ..., 10
```

> **v0.1:** Only `T[]` and `Range` are supported as iterables. User-defined
> `Iterable<T>` implementations are a v0.2 feature (requires the trait system).

### Loop

`loop` creates an infinite loop. It is the only loop form that can produce a value:

```moonlane
loop {
    // runs forever unless break is used
}

let result = loop {
    if (condition) { break value; }
};
```

**Typing rules:**

- `loop { break expr; }` has type `T` where `expr: T`. All `break` arms must produce the same type.
- `loop { }` — a loop with no reachable `break` — has type `!` (Never). See [Never Type](types.md#never-type).

### Break and Continue

`break` exits the innermost loop. `break expr` exits a `loop` and produces `expr` as the loop's value.

`continue` skips to the next iteration of the innermost loop.

### Return

```moonlane
return;         // from a function returning ()
return value;   // from a typed function
```
