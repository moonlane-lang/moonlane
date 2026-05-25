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
        (area as Int).to_string()
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
    Shape::Circle { radius } => println(radius),
    Shape::Rectangle { width, height } => println(width),
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

**Braceless bodies.** A single expression may be used as the branch body without braces:

```moonlane
if (debug) print_state();                    // statement position
let x = if (flag) value_a else value_b;     // expression position
```

The braceless form desugars to a single-expression block. Three restrictions apply:

1. **Arm style must be consistent.** Both the `then` and `else` arms must use the same style — either both braced or both braceless. Mixing is a parse error.
2. **Dangling-else is forbidden.** If the outer body is braceless, the body expression must not itself be an `if–else`. Use braces on the outer body to resolve the ambiguity.
   ```moonlane
   if (a) if (b) expr;          // ok: inner if has no else
   if (a) if (b) x; else y;    // parse error: wrap outer body in braces
   if (a) { if (b) x; else y; } // ok
   ```
3. **No semicolon between braceless arms.** Write `if (c) a else b;`, not `if (c) a; else b;` — the `;` terminates the statement before the `else`.

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

`for-in` works on any type implementing the `Iterable<T>` aspect. The loop variable
receives type `T`. `T[]` (array) and `Range` (produced by `..` and `..=`) implement
`Iterable<T>` by default. User-defined types can be made iterable by implementing
`Iterable<T>`:

```moonlane
aspect Iterable<T> {
    fun next(mut self) -> Perhaps<T>;
}
```

```moonlane
for (let item in collection) { ... }
for (let i in 0..10) { ... }    // 0, 1, ..., 9
for (let i in 0..=10) { ... }   // 0, 1, ..., 10
```

> **v0.1–v0.3:** Only `T[]` and `Range` are supported as iterables. User-defined
> `Iterable<T>` implementations are a v0.4 feature.

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
