---
id: rfc-0024
title: "Linear Types"
date: '2026-05-24'
status: draft
target:
---

## Summary

Add opt-in linear types to Gust. A value whose type is declared `linear` must be used **exactly once** — not silently dropped, not used twice. Linearity is checked statically as a second pass after type inference, with no runtime overhead. A narrow read-reference form `&T` (expression-only, non-storable) allows inspection without consumption. The default runtime-managed memory model is unchanged.

---

## Motivation

Gust's default memory model is runtime-managed (reference counting). This is ergonomic for most code, but insufficient for systems-level use cases where:

- A resource must be explicitly released (file handles, sockets, buffers)
- Allocation and deallocation must be deterministic and zero-overhead
- Use-after-free and resource leaks should be caught at compile time, not at runtime

Linear types provide this without requiring the full ownership and borrow-checker machinery of Rust. The programmer opts in per type; all other code is unaffected.

---

## Proposal

### 1. Declaring linear types

The `linear` keyword annotates a `struct` or `enum` declaration:

```gust
linear struct Buffer {
    ptr: Int,
    len: Int,
}

linear struct FileHandle {
    fd: Int,
}

linear enum Connection {
    Open { socket: Int },
    Closed,
}
```

Any value whose static type is `linear` is subject to the use-exactly-once rule. Non-linear types are unaffected.

A struct or enum that contains a `linear` field is itself treated as linear automatically. The `linear` keyword need not (and should not) be repeated on the outer type — it is inferred transitively:

```gust
struct Request {
    body: Buffer,    // Buffer is linear → Request is implicitly linear
    url: String,
}
```

### 2. Consumption

A linear value is **consumed** by any of:

- Passing it as an argument to a function
- Returning it from a function or block
- Binding it to a new name via `let` (the original binding becomes dead)
- Destructuring it in `match` or a `let` destructure

Consuming a linear value that has already been consumed is a compile error. A linear binding that reaches the end of its scope without being consumed is a compile error.

```gust
let f = FileHandle::open("data.txt");
f.close();   // consumed — ok

let f2 = FileHandle::open("data.txt");
// scope ends — ERROR: f2 not consumed

let f3 = FileHandle::open("data.txt");
f3.close();
f3.close();  // ERROR: f3 already consumed
```

### 3. Read references — `&T`

Without a way to inspect a linear value without consuming it, every method call would destroy the value. Full lifetime-tracked borrow checking is deliberately out of scope for this RFC. Instead, a minimal read reference `&T` is introduced with strict placement rules that make lifetimes unnecessary:

- `&T` is formed with the `&` prefix operator: `&expr`
- `&T` may only appear in **expression position** — it cannot be bound to a `let`, stored in a struct field, or appear in a function return type
- `&T` is not itself linear — it may be used any number of times within its expression scope
- A function that accepts `&T` may read from the value but cannot consume it (it does not own it)

```gust
linear struct Buffer { ptr: Int, len: Int }

fun buf_len(b: &Buffer) -> Int { b.len }

let buf = Buffer::alloc(1024);
let len = buf_len(&buf);   // buf is not consumed; &buf is a temporary read view
buf.free();                // consumed here
```

Because `&T` cannot be stored, it cannot outlive the expression it appears in. No lifetime annotations are needed.

Mutable references are out of scope for this RFC. Mutation of a linear value is done by consuming it and producing a new one (or by methods that take `self` and return `Self`).

### 4. Branching

Every branch of an `if` or `match` expression must leave all in-scope linear values in the same consumption state at the merge point. If a linear value is consumed in one branch, it must be consumed in all branches:

```gust
let buf = Buffer::alloc(1024);

if condition {
    buf.free();
    // ERROR: buf consumed here but not in the false branch
}

// Correct:
if condition {
    buf.write(data);
    buf.free();
} else {
    buf.free();
}
```

This rule applies to all arms of a `match` expression identically.

### 5. Loops

A linear value created **outside** a loop body may not be consumed inside it. The consumption count would be unpredictable (zero iterations, one, or many):

```gust
let buf = Buffer::alloc(1024);
for item in items {
    buf.write(item);  // ERROR: buf created outside; cannot consume in loop body
}
```

A linear value created **inside** a loop body is fine — it is created and consumed once per iteration:

```gust
for item in items {
    let conn = Connection::open(item.addr);
    conn.send(item.data);
    conn.close();   // ok — created and consumed within the same iteration
}
```

### 6. `drop` — explicit discard

To consume a linear value intentionally without performing any operation, use the built-in `drop`:

```gust
let buf = Buffer::alloc(1024);
drop(buf);   // consumed; satisfies the linearity checker
```

`drop` has the signature `fun<T: Linear>(val: T)`. If `T` defines a destructor method by convention (e.g. `free`, `close`), `drop` does **not** call it — the programmer must call the destructor explicitly. `drop` is purely a linearity-checker escape hatch.

### 7. Destructuring linear types

Destructuring a linear value in `let` or `match` consumes the outer value and introduces each field as a new binding. Each extracted linear field must itself be consumed:

```gust
let Request { body, url } = req;   // req consumed; body is a new live linear binding
body.free();                        // body consumed
// url: String — non-linear, no constraint
```

Partially destructuring a linear struct (binding some fields and ignoring others with `_`) is only valid if the ignored fields are non-linear. Ignoring a linear field is a compile error:

```gust
let Buffer { ptr, .. } = buf;   // ERROR if len is linear or if Buffer has linear fields not bound
```

### 8. Runtime interaction

Linear values bypass the reference-counting runtime entirely. No `Rc` wrapper is allocated; no reference count is maintained. The backing resource is managed solely by the consuming function (e.g. `free`, `close`). The evaluator treats linear values as plain values — correctness is entirely a static guarantee.

### 9. Typechecker changes

A **linearity environment** (`LinearEnv`) is maintained alongside the existing type environment. It maps each in-scope binding to one of:

- `Unconsumed` — the value exists and has not yet been used
- `Consumed(location)` — the value was consumed at the given source location

The linearity pass runs after type inference (Pass 2), once all types are concrete and it is known which types are linear.

Rules:

| Event | Action |
|---|---|
| `let x = <linear expr>` | Add `x → Unconsumed` to `LinearEnv` |
| Use of `x` where `x` is linear | If `Unconsumed`: mark `Consumed(here)`. If `Consumed`: error — double use |
| `&x` (read reference) | Do not mark consumed; verify `x` is `Unconsumed` |
| Scope exit | For each linear binding in scope: error if still `Unconsumed` |
| `if`/`match` merge | Verify `LinearEnv` state is identical across all branches |
| Loop body entry | Snapshot linear bindings from outer scope; forbid consuming any of them inside the body |

---

## Alternatives Considered

### Full ownership + borrow checking (Rust model)

Provides the strongest static guarantees but requires lifetime annotations, mutable references, and a borrow checker that understands aliasing. This is a significant language-level investment and changes the feel of the language for all users, not just those opting into manual memory management. Deferred indefinitely.

### `Owned<T>` wrapper type

A library type `Owned<T>` wraps a value and requires explicit `.free()`. Simpler than linear types but enforced only by convention — the compiler does not verify that `.free()` is called, making leaks and double-frees possible. Linear types provide the same ergonomic opt-in with static verification.

### Region/arena allocation

Allocate from a `Region`; all values in the region are freed together when the region is freed. Complementary to linear types rather than an alternative — a region could itself be a linear value. Regions avoid per-object tracking but cannot express single-object deterministic release. Proposed as a future RFC.

### `unsafe` blocks

Gate raw memory operations behind an `unsafe` boundary, as in Rust. Rejected by the author — the goal is fine-grained control without requiring unsafe code, preserving a uniform safety story.

---

## Open Questions

1. **Destructor protocol.** Should the language define a `Drop` trait with a `drop(self)` method that is called automatically when a linear value would otherwise go out of scope unconsumed — converting a compile error into an implicit call? This would ease migration but weakens the "must be explicit" guarantee.

2. **`&T` mutability.** This RFC introduces only read references. Should a future RFC add `&mut T` with the restriction that only one `&mut` may exist at a time (enforced at the call site, not via lifetimes)? This would allow in-place mutation without consuming and reconstructing the value.

3. **Linear type parameters.** Can a generic type parameter be constrained to linear: `fun<T: Linear>(val: T)`? This RFC assumes yes (it is needed for `drop`), but the interaction with v0.2 generics needs careful design.

4. **Transitivity warnings.** When a non-annotated struct becomes implicitly linear because of a linear field, should the compiler emit a warning or require an explicit `linear` annotation on the outer struct? Implicit propagation is convenient but may surprise users.

5. **Error recovery.** When a linear value is not consumed, should the compiler attempt to insert a `drop` call automatically and emit a warning rather than a hard error? This would make the system more lenient for early-stage code.

---

## Timing Recommendation

Linear types depend on generics (v0.2, RFC-0024 needs `fun<T: Linear>`). Target **v0.3** after generics and traits are stable. The `&T` read reference form should be designed in coordination with any future mutable reference RFC to avoid syntax conflicts.

---

## References

- Language spec: `docs/public/spec.md`
- Type system spec: `docs/public/spec/types.md`
- Typechecker notes: `gust-interpreter/docs/typechecker.md`
- Related: RFC-0003 (concurrency model), RFC-0006 (closure capture semantics)
- Prior art: Linear Haskell (Bernardy et al. 2018), Rust ownership model, Cyclone regions
