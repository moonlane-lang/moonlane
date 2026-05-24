---
id: rfc-0006
title: "Closure Capture Semantics and Cross-Closure Reference Sharing"
date: '2026-05-21'
status: draft
---

## Summary

Define how closures capture values from their enclosing scope, what mechanisms exist for two closures to share the same mutable value, and what constraints apply when a closure crosses a fiber boundary. The current PoC uses clone-at-definition capture everywhere; this RFC establishes the intended permanent semantics.

---

## Motivation

The PoC evaluator captures all free variables by cloning them at closure definition time. This is correct for independent closures but makes two important patterns impossible:

**Shared mutable state between closures:**
```moonlane
// Intended: inc and get both operate on the same counter.
// Under clone capture: each holds its own copy — inc's mutations are invisible to get.
mut counter = 0;
let inc = fun() -> () { counter += 1; };
let get = fun() -> Int { counter };
```

**Mutation visible to the enclosing scope:**
```moonlane
// Intended: calling double() updates the original.
// Under clone capture: double works on a copy.
mut x = 5;
let double = fun() -> () { x *= 2; };
double();
// x is still 5 here
```

These are genuine use cases; every production language with closures solves them somehow. Moonlane needs a principled answer before the PoC evaluator is rewritten.

The answer is constrained by two upstream RFCs:

- **RFC-0001** introduces `*T` / `*mut T` — typed, explicit, non-owning references with `Rc<RefCell<T>>` backing in the evaluator.
- **RFC-0003** establishes that `*T` and `*mut T` are not `Send`, meaning a closure that holds a pointer cannot cross a fiber boundary without wrapping in `Arc<Mutex<T>>`.

This RFC decides whether reference sharing between closures should be implicit (Go-style) or explicit (via pointer types from RFC-0001), and what the `spawn { }` interaction looks like.

---

## Design Space

Three axes define the problem:

### Axis 1 — Capture semantics: by value vs by reference

**By value (current PoC):** at closure definition, each free variable is cloned. The closure owns a private copy. Mutations inside the closure do not affect the outer binding; mutations to the outer binding do not affect the closure.

**By reference (Go model):** the closure holds an implicit pointer to the enclosing scope's binding. Mutations are visible in both directions. This is the source of Go's classic closure-over-loop-variable bug (`for i := range xs { go func() { use(i) }() }` where all goroutines see the final value of `i`).

**Explicit pointer capture (RFC-0001 model):** value capture by default; reference capture requires the programmer to explicitly take a pointer before closing:
```moonlane
mut counter = 0;
let p = &mut counter;
let inc = fun() -> () { *p += 1; };
```
Aliasing is visible at the capture site. The loop variable problem cannot occur silently.

### Axis 2 — Shared ownership: can two closures alias the same value?

With by-value capture: impossible without a shared container type.
With by-reference capture: automatic — both closures close over the same binding.
With explicit pointer capture: possible if both closures capture the same pointer `p`.
With a reference-counted container (`Rc<RefCell<T>>`): possible — both closures clone the `Rc`, sharing the inner value.

### Axis 3 — Fiber boundary: what does `spawn { }` require?

RFC-0003 requires all values captured by a `spawn { }` block to be `Send`. RFC-0001 makes `*T` and `*mut T` non-`Send`. The consequence:

- A closure with by-reference captures (implicit or explicit pointers) cannot be spawned without wrapping in `Arc<Mutex<T>>`.
- A closure with by-value captures (clones of `Send` types) can always be spawned.
- A closure holding `Rc<RefCell<T>>` cannot be spawned (`Rc` is not `Send`).
- A closure holding `Arc<Mutex<T>>` can be spawned (`Arc<Mutex<T>>` is `Send` when `T: Send`).

---

## Proposal

### Default: value capture (clone)

Closures capture by value. At definition time, every free variable that appears in the closure body is cloned into the closure's environment. This is the current PoC behaviour and becomes the permanent default.

Rationale:
- Consistent with Moonlane's existing value semantics (struct assignment clones).
- No implicit aliasing — the programmer always sees a clone at the definition site.
- `spawn { }` compatibility: value-capture closures are spawnable when all captured types are `Send`.
- Eliminates the loop variable bug class entirely.

### Sharing state: via explicit pointer types (RFC-0001)

To share mutable state between two closures, the programmer takes an explicit pointer before closing over it:

```moonlane
mut counter = 0;
let p: *mut Int = &mut counter;
let inc = fun() -> () { *p += 1; };
let get = fun() -> Int { *p };
inc();
inc();
let n = get();  // n == 2; counter == 2
```

Both `inc` and `get` capture `p` by value (they hold a cloned `*mut Int`). Cloning a `*mut Int` produces a second pointer to the same `Rc<RefCell<Int>>` cell — this is how reference semantics are achieved under value-capture rules.

This means: **pointer types are their own aliasing mechanism**. Cloning a `*T` produces a second read-only alias; cloning a `*mut T` produces a second mutable alias. The programmer opts in explicitly by taking a pointer.

This avoids a special capture mode — the language has one capture rule (clone) and one aliasing mechanism (pointers). The interaction between the two produces shared-reference closures without adding a new language concept.

### Cross-fiber closures: explicit `Arc<Mutex<T>>`

A closure passed to or spawned by `spawn { }` must have all captures satisfy `Send`. Since `*T` and `*mut T` are not `Send`, a closure with pointer captures cannot be spawned directly.

For shared mutable state across fiber boundaries, wrap in `Arc<Mutex<T>>`:

```moonlane
let counter: Arc<Mutex<Int>> = Arc::new(0);
let c1 = counter;   // Arc::clone — shares the same Mutex
let c2 = counter;
spawn { *c1.lock() += 1; };
spawn { *c2.lock() += 1; };
```

`Arc<Mutex<T>>` is `Send` when `T: Send`, satisfying the `spawn { }` constraint. Shared mutable state across fibers is always behind a mutex — there is no non-`Send` escape hatch.

### Optional `send` qualifier on function types

Under the proposed model (clone capture + explicit `*mut T`), the only capture property that matters to callers is `Send`-ness: can this closure be passed across a fiber boundary? The `FnMut`/`FnOnce` split is not needed because:
- Mutation of captured values goes through pointer indirection, not through `&mut self` on the closure.
- Clone-captured values are owned by the closure and never consumed — every closure is callable multiple times.

The single dimension is therefore: **does this closure hold any non-`Send` captures?**

**Syntax:** an optional `send` qualifier on the function type:

```moonlane
fun(T) -> R         // unqualified — captures may or may not be Send
send fun(T) -> R    // qualified — all captures are guaranteed Send
```

Free functions (no captures) are always `send fun`. A closure literal is inferred as `send fun` when all its captured values are `Send`, and as `fun` otherwise.

**Subtyping:** `send fun(T) -> R` is a subtype of `fun(T) -> R`. A `send fun` can be used anywhere a plain `fun` is expected; the reverse is not permitted. This is consistent with the rest of the `Send` system — a more capable type coerces down to a less capable one, but not up.

**Inference:** the compiler infers the most precise type the closure satisfies. If all captures are `Send`, the inferred type is `send fun`; otherwise `fun`. The programmer may also write the qualifier explicitly to get a compile-time check:

```moonlane
// Inferred as send fun — counter: Int is Send
let inc: send fun() -> () = fun() -> () { counter += 1; };

// Inferred as fun — p: *mut Int is not Send
let bump: fun() -> () = fun() -> () { *p += 1; };

// Explicit annotation — compile error if captures are not Send
let safe: send fun() -> Int = fun() -> Int { captured_int };
```

**Parameter types and API boundaries:** a function that accepts a closure can declare whether it requires `Send`-ness:

```moonlane
// Accepts any closure — cannot guarantee spawnability
fun run(f: fun() -> Int) -> Int { f() }

// Only accepts Send closures — can safely spawn
fun run_parallel(f: send fun() -> Int) -> Int {
    spawn { f() };
    0
}
```

**`spawn { }` interaction:** `spawn { }` requires all captured values to be `Send`. A closure-typed capture must therefore be `send fun`. This is checked statically: capturing a plain `fun` inside a `spawn` block is a type error.

**`Send` propagation through structs:** a struct field of type `fun() -> R` makes the struct non-`Send` (conservative — the closure's captures are unknown to the type system). A struct field of type `send fun() -> R` does not block `Send` derivation; the struct can still be `Send` if all other fields are `Send`.

---

## Alternatives Considered

### A — Implicit reference capture (Go model)

All closures close over references to the enclosing scope. Mutations are always shared.

**Rejected.** Implicit aliasing violates Moonlane's design principle of "no implicit conversions." The loop variable bug is a well-documented footgun. Cross-fiber spawning of closures with reference captures requires the programmer to think about `Send` for every closure; Go simply has no such protection and races are possible. Explicit pointer capture makes aliasing visible at the definition site.

### B — `move` / non-`move` closure distinction (Rust model)

Two syntactic forms:
- `fun(...) -> R { ... }` — reference capture (borrows from enclosing scope)
- `move fun(...) -> R { ... }` — value capture (moves/clones from enclosing scope)

In Rust, `spawn` requires `move` closures because thread-local references cannot cross thread boundaries.

**Rejected.** Moonlane does not have borrow checking or lifetimes. A reference-capture closure whose enclosing scope has ended would dangle — there is no mechanism to prevent this at compile time. Without borrow checking, implicit reference capture is unsound. The explicit-pointer approach (Proposal) gives the same expressive power with safety enforced by the `*T`/`*mut T` type system rather than a borrow checker.

### C — `Rc<RefCell<T>>` as primary sharing primitive (no pointer syntax)

Shared mutable state is always wrapped in `Rc<RefCell<T>>` directly. No pointer types in the language — RFC-0001 is deferred or dropped.

**Partially rejected.** `Rc<RefCell<T>>` is the right tool for heap-allocated shared ownership. However, it requires a standard library type to express what pointers express at the language level. RFC-0001's explicit `&` / `&mut` syntax is more ergonomic for the common case of sharing a stack-local value between closures in the same scope. Both mechanisms should coexist: `*T` for short-lived intra-scope sharing, `Rc<RefCell<T>>` for heap-allocated long-lived sharing.

### D — `Arc<RefCell<T>>` / no `Rc` distinction

Eliminate `Rc` entirely. Use `Arc` everywhere (atomic reference count, `Send`-safe).

**Rejected for PoC.** `Arc` has higher runtime overhead than `Rc` due to atomic operations. In a single-fiber program, `Rc` is always sufficient and cheaper. The `Rc` vs `Arc` split exists precisely to avoid paying for cross-thread safety when it is not needed. RFC-0003 already established this distinction.

---

## Interaction with Upstream RFCs

| RFC | Dependency |
|---|---|
| RFC-0001 (Pointers) | This RFC depends on `*T`/`*mut T` for the explicit-sharing proposal. RFC-0001 must be accepted before this RFC can be closed. In particular, RFC-0001's open question about `&x` on non-variable expressions (Q3) affects whether `&mut counter` inside a closure definition is valid. |
| RFC-0003 (Concurrency) | This RFC depends on RFC-0003's `Send` marker for the `spawn { }` constraint. The rule "`*T`/`*mut T` are not `Send`" is defined there and referenced here. RFC-0003 should be accepted first. |
| RFC-0002 (Trait Bounds) | `send fun` as a generic type parameter bound (`<F: send fun(T) -> R>`) requires RFC-0002's bound syntax to support the `send` qualifier on function types. Not blocking for capture semantics themselves, but must be resolved before the generics implementation. |

---

## Open Questions

1. **`Fn` / `FnMut` / `FnOnce` — is a single `fun` type sufficient?**

   ~~Deferred.~~ **Addressed by the `send fun` proposal above.** Under clone capture + explicit pointers, `FnMut` and `FnOnce` are not needed — the `send`/non-`send` distinction is the only dimension that matters to callers. The remaining open sub-question is how `send fun` interacts with generic bounds from RFC-0002: can you write `<F: send fun(Int) -> Int>` as a type parameter bound? The answer depends on RFC-0002's bound syntax; the `send` qualifier should be expressible as a bound.

2. **Lifetime of pointer captures**

   A closure capturing `p: *mut Int` where `p` points to a stack-local `counter` — what happens if `counter` goes out of scope before the closure is called? In the current `Rc<RefCell<Value>>` evaluator model, the `Rc` keeps the cell alive, so this is actually safe at runtime (the cell outlives both the original binding and the closure). But this is not obvious to the programmer: `counter` appears to be gone, yet `*p` still works. This is Go's behaviour (closing over a variable keeps it alive on the heap). The RFC should document whether this lifetime extension is intended, and whether the spec should say so explicitly.

3. **Two closures capturing the same `*mut T` — write ordering**

   If `inc` and `get` both hold `p: *mut Int`, and are called interleaved in a single-fiber program, the behaviour is deterministic (sequential). But if two closures capturing the same `*mut T` are both passed to an API that calls them concurrently — is this possible? RFC-0003 says `*mut T` is not `Send`, so this cannot happen across fiber boundaries. Within a single fiber, closures are called sequentially. The question is whether the type system should enforce this or whether it is left to the programmer's discipline. Propose: leave to the programmer within a single fiber; the `Send` constraint handles the cross-fiber case.

4. **Closures as struct fields — capture and `Send`**

   ~~Deferred.~~ **Addressed by the `send fun` proposal above.** A struct field typed `fun() -> R` is conservatively non-`Send` (captures unknown). A struct field typed `send fun() -> R` does not block `Send` derivation. The programmer chooses the qualifier at the struct field declaration; the typechecker enforces it at the assignment site. The remaining open sub-question is whether the typechecker should warn when a struct field is `fun() -> R` and the struct is otherwise fully `Send` — such a field may be unintentionally preventing the struct from being spawnable.

5. **`send` annotation at module/library boundaries**

   When a library exports a function that returns a closure, the `send`-ness of the returned function type is part of the public API contract. Two policies are possible:

   **Option A — Inferred from body (fragile):** the compiler propagates the closure's inferred `send`-ness to the signature. If the implementation adds a non-`Send` capture, the public type silently degrades from `send fun` to `fun` — a breaking change with no visible signal in the signature.

   **Option B — Unannotated defaults to `fun` (non-Send) at signature boundaries (recommended):** `send`-ness is only propagated inside function bodies. In any exported function signature, an unannotated `fun(T) -> R` is non-`Send`. Authors who want to promise spawnability must write `send fun(T) -> R` explicitly. The contract is then visible and stable:
   - `fun` → `send fun`: additive, non-breaking (callers gain capability)
   - `send fun` → `fun`: visibly breaking (callers lose capability and the annotation disappears)

   This is consistent with Rust's rule that `pub fn` signatures cannot rely on inference for types that appear in public position, and with Moonlane's "no implicit conversions" principle — `Send`-ness at API boundaries should be a deliberate declaration, not a side-effect of implementation details.

   A lint warning when an exported signature uses `fun` but the body is provably `send fun` would help authors discover the upgrade opportunity without making it a hard error.

   Resolution of this question requires the module system (not yet designed). It should be revisited when the module/visibility RFC is written.

6. **`Rc<RefCell<T>>` vs `*mut T` — when to use which?**

   Both enable shared mutable state within a single fiber. The distinction:
   - `*mut T` is a thin pointer to an existing binding; binding lifetime must (in principle) exceed the pointer. The Rc backing keeps it alive, but the intended idiom is short-lived intra-scope sharing.
   - `Rc<RefCell<T>>` is a heap-allocated, reference-counted cell with no originating binding. The intended idiom is longer-lived or ownership-transferred shared state.
   
   A style guide recommendation should be part of the language documentation before this RFC is closed.

---

## Timing Recommendation

This RFC should be resolved **before the PoC evaluator is rewritten**. The rewrite is the right moment to change capture semantics, since the `Value` and `Environment` types need changes anyway. The PoC's clone-capture model is correct for the test suite as-is and does not need changing before the rewrite.

The blocking dependency is RFC-0001 (pointer types). Resolve RFC-0001 before implementing the capture changes in the rewrite.

RFC-0003 (concurrency) is informative here but not blocking for the capture semantics change itself — `spawn { }` is not implemented in the PoC. The `Send` constraint on `spawn { }` captures can be added alongside concurrency implementation.

---

## References

- Language spec: [`spec/functions.md#closures`](../../public/spec/functions.md#closures), [`spec/runtime.md#panics`](../../public/spec/runtime.md#panics)
- RFC-0001: `docs/internal/rfcs/rfc-0001-pointer-syntax.md` — `*T`/`*mut T`, `Rc<RefCell<Value>>` evaluator backing, non-`Send` classification
- RFC-0003: `docs/internal/rfcs/rfc-0003-concurrency-model.md` — `Send` marker, `spawn { }` capture constraints, `Arc<Mutex<T>>`
- RFC-0002: `docs/internal/rfcs/rfc-0002-trait-bound-syntax.md` — trait bounds on `fun` types (open question 1 and 4)
- RFC-0024: `docs/internal/rfcs/rfc-0024-linear-types.md` — linear values cannot be clone-captured; move capture (`move fun`) is required; linear values can be passed as explicit closure parameters
- RFC-0025: `docs/internal/rfcs/rfc-0025-region-allocation.md` — `Region` handles are linear; move capture or explicit parameter passing required
- RFC-0026: `docs/internal/rfcs/rfc-0026-unsafe-blocks.md` — inside an `unsafe fun` closure, the linear capture restriction is relaxed; `unsafe fun` closures are never inferred as `send fun`
- Cluster report: `docs/internal/rfc-cluster-memory-model.md`

---

## Decision

**Outcome:** *(pending)*  
**Target:** *(blank until accepted)*

*(Decision rationale goes here when the RFC is evaluated.)*
