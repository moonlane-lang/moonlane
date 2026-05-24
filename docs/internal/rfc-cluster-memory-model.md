# RFC Cluster: Memory Model

**Status:** Under resolution  
**Tracking issue:** #118  
**RFCs in scope:** RFC-0001, RFC-0003, RFC-0006, RFC-0024, RFC-0025, RFC-0026

---

## Overview

Four open RFCs collectively define Gust's memory and concurrency model. They were written independently but are deeply interdependent — accepting or implementing any one of them without resolving the others will produce inconsistencies that require breaking changes later. This document maps the conflicts, establishes the decisions that must be made, and proposes a resolution order.

The four RFCs:

| RFC | Title | Status | Current target |
|---|---|---|---|
| RFC-0001 | Pointer Syntax and Semantics | Deferred | v0.3 |
| RFC-0003 | Concurrency Model | Draft | v0.4 |
| RFC-0006 | Closure Capture Semantics | Draft | — |
| RFC-0024 | Linear Types | Draft | v0.3 |
| RFC-0025 | Region Allocation | Draft | v0.4 |
| RFC-0026 | Unsafe Blocks | Draft | v0.4 |

---

## Dependency Graph

```
RFC-0024 (Linear Types)
    │
    ├──► conflicts with ──► RFC-0001 (Pointers)
    │        │
    │        └──► depended on by ──► RFC-0006 (Closure Capture)
    │                                    │
    │                                    └──► depends on ──► RFC-0003 (Concurrency)
    │
    ├──► constrains ──► RFC-0003 (Concurrency)
    │
    ├──► required by ──► RFC-0025 (Region Allocation)
    │        │
    │        └──► escape hatch from ──► RFC-0026 (Unsafe Blocks)
    │
    └──► escape hatch from ──► RFC-0026 (Unsafe Blocks)
             │
             └──► also escapes ──► RFC-0001, RFC-0003, RFC-0006
```

RFC-0006 explicitly lists RFC-0001 and RFC-0003 as blocking dependencies. RFC-0024 introduces constraints on both RFC-0001 (aliasing) and RFC-0006 (capture). RFC-0003 is the most independent but is constrained by both RFC-0001 and RFC-0024.

---

## Conflict Analysis

### Conflict 1 — The `&` syntax collision (RFC-0001 vs RFC-0024)

RFC-0001 proposes `&x` as the address-of operator, producing a storable, RC-backed `*T` pointer:

```gust
mut x: Int = 42;
let p: *Int = &x;       // p is a *Int — storable, cloneable, RC-backed
let q: *mut Int = &mut x;
```

RFC-0024 proposes `&T` as a read reference — a non-storable, expression-only view used to inspect a linear value without consuming it:

```gust
let buf = Buffer::alloc(1024);
let len = buf_len(&buf);   // &buf is a temporary — cannot be stored, cannot outlive the expression
```

The same sigil means different things with incompatible semantics. This is not a minor syntactic overlap — `*T` (RFC-0001) is reference-counted and storable, `&T` (RFC-0024) is non-storable and has no runtime representation. Shipping both independently creates a language where `&x` means two different things depending on context.

**Decision required:** choose one of:

- **Option A — Differentiate the sigils.** Keep `&` for RFC-0001's address-of. Give RFC-0024's read reference a distinct sigil (e.g. `@T`, `ref T`, or a keyword like `peek(x)`).
- **Option B — Unify under `&`.** Drop RFC-0001's `*T` / `*mut T` entirely for linear types. `&x` always means "a non-owning view"; for non-linear types, `&x` is still RC-backed and storable; for linear types, `&x` is expression-only. The type system distinguishes based on whether the target type is linear.
- **Option C — Restrict RFC-0001 to non-linear types only.** `&x` produces `*T` for non-linear `x`, and a RFC-0024-style read reference for linear `x`. The same sigil, but the result type differs. This is implicit and potentially confusing.

Option B is the most coherent: the `&` sigil consistently means "non-owning view," and the distinction between storable (non-linear) and non-storable (linear) falls out of the type system rather than from different syntax.

---

### Conflict 2 — Aliasing vs linearity (RFC-0001 vs RFC-0024)

RFC-0001's mechanism for sharing state between closures is: take a pointer, then clone the pointer. Cloning `*mut T` produces a second mutable alias to the same RC cell. This is the entire point of pointers in RFC-0001's model.

Linear types require the opposite: a linear value has exactly one owner. A second alias is a violation of the invariant — two aliases mean two potential consumers, breaking the exactly-once guarantee.

**Consequence:** `*T` and `*mut T` cannot point to linear values. Attempting to take `&x` where `x` is linear must be a type error under RFC-0001's semantics. This is not a small restriction — it means the two features operate in completely separate worlds: pointers for non-linear (RC-managed) values, and RFC-0024's narrow `&T` for linear values.

**Decision required:** whether this hard separation is acceptable or whether a unified mechanism (e.g. "tracked unique pointer" — a `*T` that the type system knows is the only alias) is needed. A tracked unique pointer would be a significant addition; the hard separation is simpler but means linear types cannot use any of RFC-0001's infrastructure.

---

### Conflict 3 — Clone capture vs linear values (RFC-0006 vs RFC-0024)

RFC-0006 proposes that closures capture all free variables by cloning at definition time. A linear value cannot be cloned — there is by definition only one copy of it. Implicit clone capture of a linear value is therefore a type error.

Explicit pointer capture (RFC-0006's workaround for shared mutable state) is also forbidden for linear values (Conflict 2 above).

This leaves no way to use a linear value from an enclosing scope inside a closure body under RFC-0006's current model.

**Missing feature:** RFC-0006 has no move-capture mechanism. A move capture would transfer the linear value into the closure at definition time, consuming it in the outer scope. This is the only sound option for closures and linear types.

**Decision required:** RFC-0006 must be amended to add move capture, at minimum for linear types. The question is whether move capture should be:

- **Linear-only** — the compiler automatically move-captures linear values; non-linear values continue to clone-capture.
- **Explicit opt-in** — a `move` qualifier on the closure (as in Rust's `move || { ... }`) transfers all linear free variables. Non-linear values can still be clone-captured.
- **Per-variable** — something like `capture(move buf, clone counter)` in the closure header. Maximum control, maximum verbosity.

The explicit opt-in (`move fun(...) { ... }`) is the most consistent with Gust's "no implicit conversions" principle. Automatic move-capture for linear values specifically is a reasonable middle ground since the linear type system already tracks whether a value is consumed.

---

### Conflict 4 — Shared ownership types vs linearity (RFC-0003 vs RFC-0024)

RFC-0003 introduces `Arc<T>` as the mechanism for sharing values across fiber boundaries. `Arc<T>` works by cloning the Arc handle, producing multiple co-owners of the inner value — the reference count determines when the value is freed.

For a linear `T`, this is unsound: `Arc<LinearT>` would allow the same linear value to be accessible from multiple Arc handles simultaneously, with no guarantee about which one "consumes" it. `Arc::clone` would violate linearity.

`Rc<LinearT>` is the same problem within a single fiber.

**Consequence:** `Arc<LinearT>` and `Rc<LinearT>` must be forbidden. The type system should reject them.

`Mutex<LinearT>` is a more nuanced case. A `Mutex` grants exclusive access to its inner value — at any point in time, only one lock holder can access the value. This is structurally compatible with linearity in intent (single accessor at a time), but the `Mutex` itself has shared ownership across its handles. The inner `T` never moves; it is accessed through a guard and then released. This is not consumption in the linear sense. **Ruling:** `Mutex<LinearT>` is forbidden — the Mutex model (permanent shared ownership of a fixed inner value) is incompatible with linear types (transient unique ownership with mandatory transfer).

**Channel send is compatible.** `ch <- value` transfers the value into the channel — this is consumption. A linear value sent through a channel satisfies the exactly-once rule: the sender no longer holds it, the receiver receives it once. Channels are the natural cross-fiber transport for linear values.

---

## Proposed Resolution Order

The RFCs must be resolved in a specific order because later decisions depend on earlier ones.

### Step 1 — Resolve the `&` syntax conflict (RFC-0001 × RFC-0024)

This is the foundational decision. All other conflicts depend on knowing what `&x` means. Until this is resolved, neither RFC-0001 nor RFC-0024 can be written in their final form.

**Recommended decision:** Option B from Conflict 1 — unify under `&`. The `&` sigil means "non-owning view." For non-linear types, `&x` is RC-backed and storable (`*T`). For linear types, `&x` is non-storable (RFC-0024's read reference). The type system distinguishes based on linearity. This keeps one mental model for `&` across the entire language.

**Output:** An amendment to both RFC-0001 and RFC-0024 documenting the unified semantics.

### Step 2 — Establish the linear/pointer boundary (RFC-0001 amendment)

With the syntax conflict resolved, add an explicit rule to RFC-0001: `&x` and `&mut x` are type errors when `x` is linear. Linear values are not addressable via `*T`. The only interaction between linear values and references is via RFC-0024's non-storable `&T`.

Document whether "tracked unique pointers" (a `*T` known to be the sole alias) are in or out of scope for v0.3.

**Output:** RFC-0001 amendment closing its open question about linear type interaction.

### Step 3 — Add move capture to RFC-0006

With the pointer boundary established, RFC-0006 can be amended with a move capture form. The recommended approach is an explicit `move` qualifier on closures:

```gust
let buf = Buffer::alloc(1024);
let process = move fun() { buf.write(data); buf.free(); };
// buf is consumed here — it has moved into the closure
process();
```

Non-linear values inside a `move fun` are still clone-captured. Linear values are move-captured (consumed). The `move` qualifier is required whenever the closure body references a linear binding from the outer scope — the compiler errors if you omit it, since neither clone nor pointer capture is valid.

**Output:** RFC-0006 amendment adding `move fun` syntax and specifying linear-value move-capture semantics.

### Step 4 — Close RFC-0003 restrictions on linear types

With linear types defined and the pointer boundary established, RFC-0003 needs explicit additions:

- `Arc<T>` and `Rc<T>` require `T: !Linear` (T must not be linear).
- `Mutex<T>` requires `T: !Linear`.
- Linear types that are `Send` can be transferred via channels — document this as the idiomatic pattern.
- Add to the `Send` derivation table: a linear type is `Send` if all its fields are `Send` (same rule as non-linear types — linearity does not affect Send-ness, only aliasing).

**Output:** RFC-0003 amendment adding the `Arc`/`Mutex` restrictions and documenting channels as the linear-value concurrency primitive.

### Step 5 — Final acceptance

All four RFCs can then be formally accepted. Their targets:

| RFC | Revised target |
|---|---|
| RFC-0001 | v0.3 |
| RFC-0024 | v0.3 (alongside RFC-0001) |
| RFC-0006 | v0.3 (closure rewrite is pre-requisite for pointer and linear type implementation) |
| RFC-0003 | v0.4 (unchanged — concurrency is post-generics) |
| RFC-0025 | v0.4 (depends on RFC-0024; pairs naturally with concurrency where region patterns emerge) |
| RFC-0026 | v0.4 (`unsafe fun` syntax locked in at v0.3 alongside RFC-0001; block implementation deferred) |

---

## Decision Log

| # | Decision | Status |
|---|---|---|
| D1 | `&` syntax: unify under one sigil or differentiate | **Open** |
| D2 | Tracked unique pointers: in or out of scope for v0.3 | **Open** |
| D3 | Move capture: linear-only automatic, or explicit `move` qualifier | **Open** |
| D4 | `Mutex<LinearT>`: forbidden or permitted with restrictions | **Proposed forbidden** — pending review |
| D5 | Linear `Send` derivation rule | **Proposed** — same as non-linear (field-based) |
| D6 | Region access: scope/callback (Option A) vs direct in `unsafe` (Option B) vs both | **Open** |
| D7 | `unsafe fun` syntax: lock in at v0.3 with RFC-0001 or defer to v0.4 | **Open** |

---

## References

- RFC-0001: `docs/internal/rfcs/rfc-0001-pointer-syntax.md`
- RFC-0003: `docs/internal/rfcs/rfc-0003-concurrency-model.md`
- RFC-0006: `docs/internal/rfcs/rfc-0006-closure-capture-semantics.md`
- RFC-0024: `docs/internal/rfcs/rfc-0024-linear-types.md`
- RFC-0025: `docs/internal/rfcs/rfc-0025-region-allocation.md`
- RFC-0026: `docs/internal/rfcs/rfc-0026-unsafe-blocks.md`
