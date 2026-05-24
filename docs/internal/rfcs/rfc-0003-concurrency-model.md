---
id: rfc-0003
title: "Concurrency Model"
date: '2026-05-20'
status: draft
---

## Summary

Define Gust's concurrency model: language-native fibers, typed channels as the primary communication primitive, a `select` expression for multiplexing, and a single `Send` marker trait to prevent data races at the type level without ownership semantics. The design follows Go's philosophy — concurrency is transparent syntactically, managed by the runtime, and idiomatic code communicates through channels rather than shared memory.

---

## Motivation

Gust's current spec has no concurrency primitives. Adding them now, before the pointer RFC (RFC-0001) is finalised, is important because the two designs are coupled: `*mut T` in a concurrent setting creates data races unless the type system or runtime prevents them. Resolving the concurrency model first lets RFC-0001 make the right choices about pointer transferability.

The three problems concurrency must address:

1. **Parallelism** — doing multiple things at once (CPU-bound work across cores)
2. **I/O multiplexing** — waiting on multiple sources without blocking an OS thread per source
3. **Coordination** — communicating results and signalling termination between concurrent tasks

The chosen model determines how complex each of these is to express and how easy it is to accidentally introduce data races or deadlocks.

---

## Design Philosophy

Go's concurrency model provides the clearest reference point for Gust's stated goals:

> *"Don't communicate by sharing memory; share memory by communicating."*
> — Rob Pike

The concrete implications for Gust:

- **No function colouring** — launching a fiber does not require functions to be declared differently (`async fn` in Rust/JavaScript). Blocking inside a fiber is fine; the runtime schedules around it. This avoids the "what colour is my function?" problem entirely.
- **Channels are the primary primitive** — values are *transferred* between fibers through typed channels, not *shared*. Fibers own their data; ownership moves when a value is sent.
- **The runtime manages scheduling** — fibers are lightweight (green threads, M:N scheduled). The programmer launches a fiber and forgets about threads, cores, and scheduling.
- **Shared mutable state is possible but opt-in** — `Mutex<T>` and `Atomic<T>` in the standard library cover the cases where shared state is genuinely necessary. These are library types, not language features.
- **One type-level rule prevents the worst races** — `*mut T` is not `Send`. This single constraint, enforced at compile time, blocks the most dangerous class of data races (shared mutable pointer across fiber boundaries) without requiring a full ownership system.

---

## Proposed Design

### Fibers

A fiber is a lightweight concurrent task launched with the `spawn` keyword:

```gust
spawn { heavy_computation(data) }
```

`spawn { ... }` is a statement. The block runs concurrently. The launching fiber continues immediately.

Any expression is valid inside `spawn { ... }`. The return value of the block is discarded unless the fiber communicates through a channel.

```gust
let ch: Chan<Int> = Chan::new();
spawn {
    let result = compute();
    ch <- result;
}
let answer = <- ch;
```

There is no handle to a fiber — fibers are fire-and-forget at the language level. Coordination happens through channels. A fiber that panics terminates the program (same as Go).

**No `async fn`/`await`:** functions are not coloured. A function that blocks inside a fiber does not need to be declared differently. The runtime detects blocking and parks the fiber until it can proceed.

---

### Channel types

`Chan<T>` is a typed, bidirectional, first-class channel. Both unbuffered and buffered variants exist:

```gust
// Unbuffered — sender blocks until receiver is ready
let ch: Chan<Int> = Chan::new();

// Buffered — sender blocks only when the buffer is full
let ch: Chan<Int> = Chan::buffered(16);
```

`Chan<T>` is `Send` for any `T: Send` — channels are designed to be passed across fiber boundaries.

**Directional subtypes** (for documentation and API clarity, not enforced at the implementation level in the PoC):

```gust
Chan<T>      // bidirectional (default)
SendChan<T>  // write-only view
RecvChan<T>  // read-only view
```

`Chan<T>` coerces to `SendChan<T>` or `RecvChan<T>` where the directional type is expected. This mirrors Go's `chan<- T` and `<-chan T` but uses named types instead of directional syntax, which is more readable and consistent with Gust's type conventions.

---

### Send and receive operators

**Send** — `ch <- value`:

```gust
ch <- 42;           // blocks if ch is unbuffered and no receiver is ready
```

Send is a statement. It moves `value` into the channel; `value` is no longer accessible in the sending fiber after this point (value semantics — the value is copied into the channel buffer, consistent with Gust's existing copy semantics for structs).

**Receive** — `<- ch`:

```gust
let x = <- ch;      // blocks until a value is available
```

`<- ch` is an expression of type `Perhaps<T>`. It evaluates to:
- `Perhaps::Some { value }` — a value was received
- `nope` — the channel is closed and drained

This models channel exhaustion without introducing a new primitive — `Perhaps<T>` already exists, and the close-signals-completion pattern maps cleanly onto it.

```gust
while let Perhaps::Some { value: x } = <- ch {
    process(x);
}
// loop exits when channel is closed and drained
```

**Close** — `ch.close()`:

Marks the channel as closed. Further sends are a runtime panic. Receivers drain any buffered values, then receive `nope`.

---

### The `select` expression

`select` waits on multiple channel operations simultaneously, executing the first one that is ready. It is an expression — every arm produces a value of the same type.

```gust
let result = select {
    v <- ch1       => process(v),
    ch2 <- payload => "sent",
    else           => "would block",   // optional: makes select non-blocking
}
```

`else` is optional. Without it, `select` blocks until one arm is ready. With `else`, `select` returns the `else` value immediately if no arm is ready (non-blocking poll).

**Receive arm**: `v <- ch` — evaluates to the received value, bound as `v` in the arm body. Arm is ready when a value is available. If the channel is closed, the arm receives `nope` and the binding has type `Perhaps<T>`.

**Send arm**: `ch <- value` — evaluates when the send completes. The arm body has no binding but produces the arm's expression value.

Semantics: if multiple arms are ready simultaneously, one is chosen at random (same as Go). This prevents starvation but means `select` with multiple ready arms is non-deterministic.

```gust
// Fan-in: merge two channels into one result
fun merge<T: Send>(a: RecvChan<T>, b: RecvChan<T>) -> Perhaps<T> {
    select {
        v <- a => Perhaps::Some { value: v },
        v <- b => Perhaps::Some { value: v },
        else   => nope,
    }
}
```

---

### The `Send` marker trait

`Send` is a marker trait — no methods, no implementations to write. A type that is `Send` can be moved across fiber boundaries (passed through a channel or captured by a `spawn { }` block).

```gust
trait Send {}
```

**Default implementations:**

| Type | `Send`? | Reason |
|---|---|---|
| `Int`, `Float`, `Bool`, `Str` | yes | primitives — copied |
| Structs with all-`Send` fields | yes | automatic |
| Enums with all-`Send` variants | yes | automatic |
| `Perhaps<T>` where `T: Send` | yes | automatic |
| `Result<T, E>` where `T, E: Send` | yes | automatic |
| `Chan<T>` where `T: Send` | yes | channels are designed to cross fiber boundaries |
| `*T` (read-only pointer) | **no** | aliased read could race with a concurrent write |
| `*mut T` (mutable pointer) | **no** | shared mutable access — data race |
| `Mutex<T>` where `T: Send` | yes | the mutex is the synchronisation mechanism |

The rule for `*T`/`*mut T` being non-`Send` is the single constraint that prevents the most dangerous concurrency bugs. Sending a `*mut T` across a fiber boundary without a mutex means two fibers can read and write the same memory concurrently — the canonical data race. By making all pointer types non-`Send` by default, the type system makes this a compile-time error rather than a runtime race.

**Using `*T` with concurrency:**

If shared read-only access is genuinely needed, wrap in a `Mutex<T>` or use `Arc<T>` (a reference-counted, `Send`-safe shared pointer — a standard library type, not a language primitive):

```gust
// Wrong: *mut T is not Send
let p: *mut Int = &mut x;
ch <- p;   // type error: *mut Int does not implement Send

// Right: Mutex<T> is Send
let m: Mutex<Int> = Mutex::new(x);
ch <- m;   // ok
```

**Deriving `Send` is automatic** for most types — the programmer does not annotate `Send` on their own structs. The compiler checks field types. Only types containing `*T` or `*mut T` are not `Send` by default.

---

### Standard library concurrency primitives

These are library types, not language features. They use pointer internals but expose a safe `Send` API:

| Type | Purpose |
|---|---|
| `Mutex<T>` | Exclusive mutable access. `.lock()` returns a guard; guard released on drop. |
| `RwLock<T>` | Shared read / exclusive write. |
| `Atomic<Int>`, `Atomic<Bool>` | Lock-free integer and boolean operations. |
| `WaitGroup` | Coordinate completion of a set of fibers (`add`, `done`, `wait`). |
| `Once` | Execute an initialiser exactly once across all fibers. |

`Mutex<T>` and `RwLock<T>` are `Send` because they wrap the synchronisation mechanism around the value. Internally they use `*mut T`, but the safe API prevents unsound access.

---

### Fiber lifecycle and structured alternatives

Go's goroutines are unstructured — a goroutine outlives its spawning scope, and the runtime only terminates when `main` returns or the program panics. There is no built-in "wait for all goroutines" primitive other than `WaitGroup` and explicit channel signalling.

**Structured concurrency** (Swift's `async let`, Kotlin's `launch { }` within a scope) ties fiber lifetime to a lexical scope. This prevents fiber leaks but requires a scope object and changes the programming model.

For Gust's first concurrency implementation, **unstructured fibers** (`spawn { }`) are proposed, matching Go's model. Structured concurrency can be layered on top as a library pattern using `WaitGroup` and channels. If experience shows that fiber leaks are a common source of bugs, a structured API (`scope { |s| s.spawn { ... } }`) can be added without changing the core primitives.

---

## Interaction with RFC-0001 (Pointers)

The `Send` marker trait resolves RFC-0001's remaining ambiguity about pointer semantics in a concurrent world:

1. **`*T` and `*mut T` are not `Send`** — pointers are local-fiber tools. Self-referential structs, tree nodes, and in-place mutation within a single fiber are the intended use cases. Cross-fiber sharing goes through channels (moving values) or `Mutex<T>` (protecting shared state).

2. **`Perhaps<*T>` is not `Send`** either — wrapping a non-`Send` type in `Perhaps` does not make it `Send`. `Perhaps<Mutex<T>>` is `Send` if `T: Send`.

3. **Auto-deref (RFC-0001 open question 2)**: The concurrency model has no bearing on this question. It remains deferred.

4. **Pointer equality (RFC-0001 open question 4)**: Also unaffected; defer.

The recommended sequencing remains RFC-0001's Option B: resolve RFC-0001 (pointer syntax) after the PoC evaluator is complete, incorporating the `Send` constraint specified here. RFC-0003 is the upstream dependency that RFC-0001 needs before being closed.

---

## Interaction with RFC-0002 (Trait Bounds)

`Send` is the first marker trait in the language. Its introduction has two implications for RFC-0002:

1. **Marker traits need a representation in the AST and type system.** A trait with no methods is valid under the current spec — it is simply a trait with an empty body. No new language feature is required.

2. **Fiber closure capture bounds:** A `spawn { }` block that captures a variable from the enclosing scope requires that variable's type to be `Send`. This is a use-site constraint, not a function signature bound — it is checked at the `spawn { }` site, not at the function declaration. The trait bound syntax RFC (RFC-0002) does not need to cover this case; it is a compiler rule, not a programmer-written bound.

---

## Alternatives Considered

### `async`/`await` (Rust, JavaScript, TypeScript, Python)

Functions are coloured: async functions must be called with `await`; non-async callers cannot. This solves structured concurrency naturally (tasks have explicit handles and lifetimes) but introduces the "what colour is my function?" problem. Every blocking operation requires an `async` propagation through the entire call stack. This complexity is inconsistent with Gust's goal of concurrency that is "easy to use and largely managed by the runtime."

**Verdict:** rejected. The function colouring problem is a significant ergonomic cost that conflicts with the stated design goal.

---

### Actor model (Erlang, Elixir, Akka)

Actors are isolated processes (no shared state at all) that communicate only via message passing. Each actor has a mailbox; messages are pattern-matched.

This model eliminates data races entirely — there is no shared memory to race on. It also maps well onto distributed systems (actor locations are transparent).

**Downsides for Gust:**
- Higher abstraction overhead than fibers — every concurrent unit is a named actor
- Requires a process supervisor tree for fault tolerance (desirable for Erlang; heavy for a general-purpose language)
- Struct values would still need copying on send (same as channels), but the model is less composable with functions and iterators

**Verdict:** the channel model is a subset of the actor model's message-passing philosophy (no shared state, communicate to coordinate) but is lighter-weight and more composable with existing language features. Channels are preferred.

---

### Rust's `Send`/`Sync` system

Rust has two marker traits:
- `Send` — type can be moved to another thread
- `Sync` — type can be shared (via `&T`) across threads (`T: Sync` iff `&T: Send`)

The `Sync` trait is what makes `Arc<T>` safe: `Arc<T>` is `Send` only if `T: Sync`.

For Gust, `Sync` would be needed if `*T` (read-only shared pointer) could be made `Send` when `T: Sync`. This mirrors Rust's treatment of `Arc<T>`.

**Decision:** defer `Sync` to a follow-up RFC. Introducing it now requires reasoning about `Arc<T>` (a standard library type not yet designed) and adds significant type-system complexity before the evaluator PoC even exists. The conservative position — all pointer types are non-`Send` — is sound and can be relaxed later. The converse (allowing `*T: Send` and discovering a soundness hole) would require a breaking spec change.

---

### CSP (Communicating Sequential Processes) with explicit process IDs

Go's channels are anonymous — you hold a reference to the channel, not to the goroutine on the other end. An alternative (Erlang, Akka) is to address messages to named processes or PIDs.

Named-process addressing enables supervision, restart, and distributed messaging. Anonymous channels enable simpler local coordination without a runtime registry.

**Verdict:** anonymous channels (Go model) are simpler for the use cases Gust targets. Named processes are a future extension point if distribution is ever a goal.

---

## Open Questions

1. **Fiber panic isolation**
   Go terminates the whole program when any goroutine panics (unless `recover()` is used). Should Gust fiber panics be isolated (actor model — one fiber dies, others continue) or program-terminating (Go model — simple but unforgiving)?
   
   The `Result<T, E>` type suggests Gust favours explicit error handling. A fiber could return a `Result<T, E>` through a channel rather than panicking. Whether panics inside fibers are always program-terminating or can be caught is unresolved.

2. **`Chan<T>` close semantics and `Perhaps<T>` receive**
   The proposal makes `<- ch` return `Perhaps<T>`. Go's closed-channel receive returns the zero value with a second `ok bool` (two-return-value idiom). `Perhaps<T>` is cleaner but means receive always allocates a `Perhaps` wrapper.
   
   An alternative: separate `chan.try_recv() -> Perhaps<T>` (non-blocking) from `<- ch` returning `T` (blocking, panics if channel closed without value). This avoids the allocation but makes the closed-channel case a runtime panic rather than a type-level signal.

3. **Directional channel types: language or library?**
   `SendChan<T>` and `RecvChan<T>` are proposed as stdlib types that `Chan<T>` coerces to. An alternative is language-level directional syntax (`chan<- T`, `<-chan T` as in Go). Language-level directional types would allow the typechecker to prevent sends on a receive-only channel at compile time. Proposed as library types for the initial design — promote to language-level if experience shows the coercion model is insufficient.

4. **`select` with timeout**
   Go's `select` with a timeout uses a `time.After(d)` channel. Should Gust provide a `Chan::timeout(duration) -> RecvChan<Unit>` stdlib function with the same pattern, or is a first-class `select` timeout arm needed?

5. **Fiber names and observability**
   Go's goroutines are anonymous at the language level but have stack traces in panics and the runtime debugger. Should Gust allow optional fiber names for debugging? (`spawn "worker" { ... }` or via a stdlib API.) Defer to tooling.

6. **`WaitGroup` ergonomics**
   `WaitGroup` is the standard Go pattern for "wait for N goroutines to finish." An alternative is a first-class `join` expression that waits for a set of fibers and collects their channel results. This would be more ergonomic than explicit `WaitGroup.add` / `WaitGroup.done` / `WaitGroup.wait` calls but requires fibers to have a first-class handle — which conflicts with the fire-and-forget model. Deferred.

7. **`Arc<T>` and `Sync`**
   If read-only shared pointers across fibers are needed (e.g. a large read-only data structure accessed from many fibers), neither `*T` (non-`Send`) nor channels (copy semantics) are efficient. `Arc<T>` (atomic reference count, `Send` when `T: Sync`) is the standard solution. Introducing `Arc<T>` requires the `Sync` marker trait. Defer to a follow-up RFC after the evaluator PoC, consistent with RFC-0001's timing recommendation.

---

## Timing Recommendation

Do not implement concurrency primitives in the current PoC evaluator (v0.1). The reasons:

1. The PoC evaluator uses `Rc<RefCell<Value>>` — single-threaded. Fibers require `Arc<Mutex<Value>>`. Retrofitting the evaluator mid-epic is out of scope.
2. The open questions (panic isolation, `Chan<T>` close semantics, directional channels) should be resolved through spec discussion before any implementation begins.
3. The PoC's purpose is to validate the core language (expressions, control flow, functions, closures). Concurrency is a separate capability layer.

**Minimum action from this RFC:** update the spec overview ([`spec.md`](../../public/spec.md#overview)) to name concurrency as a first-class design principle and note that language-native fibers and channels are planned. This sets expectations and prevents spec-inconsistent implementation choices in the PoC.

**Implementation target:** v0.4 (Concurrency), to be scoped after v0.1–v0.3 complete. Prior to that version, open a follow-up RFC for `Arc<T>` and `Sync` (depends on the pointer and trait implementations from v0.3–v0.4 to reason about concretely).

---

## References

- Language spec: [`spec.md`](../../public/spec.md#overview) (overview and design principles)
- RFC-0001: `docs/internal/rfcs/rfc-0001-pointer-syntax.md` — `*T`/`*mut T` as non-`Send`; timing interaction
- RFC-0002: `docs/internal/rfcs/rfc-0002-trait-bound-syntax.md` — `Send` as marker trait; fiber capture bounds
- v0.1: #1–#4 (Evaluator — PoC must complete before concurrency implementation begins)
- RFC-0024: `docs/internal/rfcs/rfc-0024-linear-types.md` — linear types are `Send` if all fields are `Send`; channel send is a natural consumption point for linear values; `Arc<LinearT>` and `Mutex<LinearT>` are forbidden
- RFC-0025: `docs/internal/rfcs/rfc-0025-region-allocation.md` — `Region` is not `Send`; region allocation is a single-fiber primitive
- RFC-0026: `docs/internal/rfcs/rfc-0026-unsafe-blocks.md` — `unsafe_send` built-in bypasses `Send` constraint inside `unsafe` blocks for lock-free data structure implementation
- Cluster report: `docs/internal/rfc-cluster-memory-model.md`
- Go specification: https://go.dev/ref/spec#Go_statements, https://go.dev/ref/spec#Select_statements
- Go memory model: https://go.dev/ref/mem

---

## Decision

**Outcome:** *(pending)*  
**Target:** *(blank until accepted)*

*(Decision rationale goes here when the RFC is evaluated.)*
