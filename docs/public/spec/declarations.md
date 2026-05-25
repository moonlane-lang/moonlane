# Declarations

## Variables

### Immutable Bindings

```moonlane
let x = 42;
let name: String = "Vlad";
```

`let` bindings cannot be reassigned and must always be initialized.

### Mutable Bindings

```moonlane
mut counter = 0;
counter = counter + 1;
counter += 1;   // compound assignment
```

`mut` bindings can be reassigned and also must be initialized at declaration. Compound assignment operators `+=`, `-=`, `*=`, `/=`, `%=` are supported.

### Scoping and Shadowing

Variables are lexically scoped. Each block `{ }` introduces a new scope. Inner scopes can shadow outer variables.

`let` and `mut` declarations are sequential — a binding is visible only from its declaration point to the end of its containing block.

`fun` declarations are hoisted to the top of their containing block. All `fun` declarations in a block are mutually visible to each other and to all other statements in that block, regardless of declaration order. This enables forward references and mutual recursion at any nesting level.

Hoisting is block-local: a `fun` declared in an inner block is not visible in the outer block. Normal lexical scoping applies across block boundaries — inner blocks see outer declarations, outer blocks do not see inner declarations.

```moonlane
fun a() { b(); }        // OK — b is hoisted within this block
fun b() { a(); }        // OK — mutual recursion at top level

fun outer() {
    inner();            // OK — inner is hoisted within outer's block

    fun inner() {
        helper();       // OK — helper is hoisted within inner's block
        fun helper() { }
    }

    helper();           // ERROR — helper is scoped to inner's block
}
```

`struct` and `enum` declarations are hoisted to **program scope** — they are visible throughout the entire program regardless of where they appear in the source. A type declared inside a function body or any nested block is as visible as a top-level type declaration. Unlike `fun` hoisting, which is block-local, type definition hoisting is global.

```moonlane
fun make_point() -> Point {
    return Point { x: 1.0, y: 2.0 };   // OK — Point is globally visible
}

fun inner() {
    struct Point {         // declared inside a function — still globally visible
        x: Float,
        y: Float,
    }
}
```

`impl` blocks follow the same global-hoisting rule as the types they extend.

---

## Structs

```moonlane
struct Point {
    x: Float,
    y: Float,
}
```

### Instantiation and Field Access

```moonlane
let p = Point { x: 1.0, y: 2.0 };
let x = p.x;
```

When a local variable has the same name as a field, the `: value` part can be omitted (**shorthand field init**):

```moonlane
let x = 1.0;
let y = 2.0;
let p = Point { x, y };   // equivalent to Point { x: x, y: y }
```

Shorthand and explicit fields may be mixed freely within one literal.

### Methods

```moonlane
impl Point {
    fun distance(self, other: Point) -> Float {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        return dx * dx + dy * dy;   // squared distance
    }
}

let d = p.distance(q);
```

`self` refers to the receiver. Methods are called with dot syntax.

### Mutable Receiver

Methods that mutate the receiver declare `mut self`. Mutation happens in place:

```moonlane
impl Counter {
    fun increment(mut self) {
        self.value += 1;
    }
}
```

### Generic Structs

```moonlane
struct Pair<A, B> {
    first: A,
    second: B,
}
```

---

## Enums

```moonlane
enum Direction { North, South, East, West }

enum Shape {
    Circle { radius: Float },
    Rectangle { width: Float, height: Float },
}
```

Variants may be unit (no data) or struct-like (named fields).

### Instantiation

```moonlane
let dir = Direction::North;
let s = Shape::Circle { radius: 5.0 };
```

### Methods on Enums

`impl` blocks on enums follow the same syntax as structs:

```moonlane
impl Shape {
    fun area(self) -> Float {
        match self {
            Shape::Circle { radius } => 3.14159 * radius * radius,
            Shape::Rectangle { width, height } => width * height,
        }
    }
}
```

---

## Aspects

> **v0.4 feature.** The aspect system is not available in v0.1–v0.3. Built-in aspect-dependent
> behaviour (`as` for `Int ↔ Float`, `?` with exact error match, `for-in` over arrays
> and ranges) is available in v0.1 as hardcoded special cases. User-defined aspects,
> `impl Aspect for Type`, and aspect bounds are v0.4.

```moonlane
aspect Printable {
    fun print(self);
}

aspect Comparable {
    fun compare(self, other: Self) -> Int;
}
```

### Implementing a Aspect

```moonlane
impl Printable for Point {
    fun print(self) {
        println("(" + self.x.to_string() + ", " + self.y.to_string() + ")");
    }
}
```

### Aspect Bounds

```moonlane
fun print_all<T: Printable>(items: T[]) {
    for (let item in items) {
        item.print();
    }
}
```

### Default Method Implementations

```moonlane
aspect Greet {
    fun name(self) -> String;

    fun greet(self) {                          // default implementation
        println("Hello, " + self.name() + "!");
    }
}
```

### The Self Type

`Self` inside a aspect definition refers to the concrete implementing type:

```moonlane
aspect Comparable {
    fun compare(self, other: Self) -> Int;
}
```

### Static Dispatch Only

Aspect objects (`dyn Aspect`) are not available in v0.1. All polymorphism is via generics (static dispatch).
