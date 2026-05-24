---
id: rfc-0025
title: "Region Allocation"
date: '2026-05-24'
status: draft
target:
---

## Summary

Introduce `Region` as a built-in linear type that provides bump-allocation from a contiguous heap block. Values allocated from a region share the region's lifetime — they are all freed together when the region is consumed. No per-object tracking, no reference counting overhead. The region handle is linear (must be explicitly consumed). Access to region-allocated values is scoped via a callback to prevent dangling references without requiring lifetime annotations.

---

## Motivation

Linear types (RFC-0024) give the programmer per-object control over allocation and deallocation. This is the right model for resources with independent lifetimes (file handles, sockets, individually managed buffers). It is a poor fit for workloads that allocate many short-lived objects that all become irrelevant at the same point in time:

- Parsing: an AST built for one source file, discarded once lowered
- Game frame: all per-frame allocations freed at end of frame
- Request handling: all per-request state freed when the response is sent
- Scratch space: temporary buffers for a computation, freed when done

For these patterns, per-object `free()` is not just redundant — it is slower than freeing the entire backing block at once. Region allocation (also called arena allocation or bump allocation) is the standard solution: allocate from the front of a contiguous block, free the whole block in one operation.

The key design constraint for Gust: without lifetime annotations, a region-allocated pointer cannot be statically verified to not outlive the region. This RFC presents two options for handling that constraint and leaves the choice as an open decision.

---

## Proposal

### `Region` as a linear type

`Region` is a built-in linear type. It must be consumed — either by calling `region.free()` or by passing it to a consuming function. It cannot be cloned or stored behind `Rc`/`Arc` (all pointer-to-linear restrictions from RFC-0024 and RFC-0001 apply).

```gust
let region = Region::new(4096);   // allocate a 4096-byte backing block
// ... use the region ...
region.free();                     // consumed; backing block is freed
```

### Option A — Scope/callback access (safe, restrictive)

Region-allocated values are accessible only inside a callback passed to `region.scope(...)`. The callback receives a reference to the region and can allocate from it. The scope returns a value that must not contain region-internal references — enforced by restricting the return type to types that are `Send` (and therefore contain no raw pointers or region-internal borrows).

```gust
let region = Region::new(65536);

let result: ParseTree = region.scope(fun(r) {
    let tokens = r.create(tokenise(source));
    let tree   = r.create(parse(tokens));
    lower(tree)           // lower() returns a ParseTree that owns its data
                          // tokens and tree are freed with the region
});

region.free();
```

`r.create(value: T) -> T` allocates `T` in the region's backing block. The `T` is usable inside the scope. Any attempt to return a value that contains a region-internal reference from the scope is a type error (because such a value would not be `Send`).

**Pros:** statically safe without lifetime annotations. **Cons:** the `Send` restriction is overly conservative — many safe values (e.g. containing `*T` from RFC-0001) cannot be returned even when they don't actually reference the region. The scope callback model is also more verbose than direct allocation.

### Option B — Programmer-responsibility access (flexible, requires discipline)

The region provides direct allocation. Region-allocated values are regular Gust values. The programmer is responsible for not using them after `region.free()` is called. This is not statically verified — it is a performance primitive that accepts the risk of use-after-free in exchange for flexibility.

```gust
let region = Region::new(65536);
let tokens = region.create(tokenise(source));
let tree   = region.create(parse(tokens));
let result = lower(tree);
region.free();
// tokens and tree are now invalid — programmer's responsibility not to use them
```

Under this option, `region.create(value: T) -> T` works outside any callback. `T` is a regular value. The region is a performance tool, not a safety guarantee.

**Pros:** ergonomic, no callback wrapping, no `Send` restriction. **Cons:** use-after-free is possible and undetected. This option requires unsafe context (see RFC-0026) to make the risk explicit.

### Recommended design direction

Option A (scope/callback) as the safe default; Option B available inside `unsafe { }` blocks (RFC-0026) for cases where the callback overhead or `Send` restriction is unacceptable. This creates a clear spectrum:

| Mechanism | Safety | Overhead |
|---|---|---|
| Linear types (RFC-0024) | Statically verified | None |
| `Region::scope` (Option A) | Statically verified (via Send bound) | Callback wrap |
| `Region::create` in `unsafe` (Option B) | Programmer responsibility | None |

### `Region` is not `Send`

The backing block is not thread-safe. `Region` does not implement `Send`. It cannot be passed through a channel or captured by a `spawn { }` block (RFC-0003). Region allocation is a single-fiber primitive.

### Interaction with `move fun` closures (RFC-0006)

A `Region` handle is linear and therefore cannot be clone-captured by a closure. It can be move-captured (`move fun`) or passed as a parameter. Move-capturing a region into a closure consumes it in the outer scope — the closure then owns the region and is responsible for freeing it.

---

## Alternatives Considered

### Per-object linear allocation (RFC-0024 only)

Linear types handle the case where objects have independent lifetimes. For batch-lifetime workloads, per-object `free()` is correct but suboptimal. Regions are a complementary mechanism, not a replacement.

### `Vec<T>` as a manual arena

A programmer could approximate an arena with a `Vec<u8>` and unsafe casting. This is error-prone and requires unsafe already. A first-class `Region` type gives the same performance with better ergonomics and a clear safety story.

### Compile-time stack allocation

For small fixed-size scratch space, stack allocation is already what the runtime does. Regions target larger, runtime-sized allocations where heap is required.

---

## Open Questions

1. **Option A vs B decision.** Is the scope/callback model (Option A) acceptable as the primary interface, or is the `Send` restriction too limiting? Should Option B be available only via `unsafe` or also via some explicit `Region::alloc_unchecked` method?

2. **`r.create(value)` semantics.** Does `create` take ownership of `value` and copy it into the region's block? Or does it allocate uninitialized memory and the value is constructed in-place? The latter is more efficient but requires a constructor callback or placement-new equivalent.

3. **Region growth.** If the region's backing block is exhausted, does `create` panic, return `Perhaps::None`, or automatically allocate a new block? A growable region (linked list of blocks) is more ergonomic; a fixed-size region is simpler and predictable.

4. **Interaction with `&T` (RFC-0024).** Inside a `scope` callback, can you take a `&T` read reference to a region-allocated value and pass it out of the scope? Under the `Send` bound this would be caught (non-Send). Under Option B this is unchecked. A specific rule may be needed.

5. **Named region types.** Should the programmer be able to define a typed region (`struct FrameArena: Region`) for documentation purposes, or is `Region` always anonymous?

---

## Timing Recommendation

Depends on RFC-0024 (linear types must be accepted first, since `Region` is a linear type). Also benefits from RFC-0026 (unsafe blocks) being at least drafted, since Option B (direct allocation) is only sound inside `unsafe`. Target **v0.4** alongside or after the concurrency work, since single-fiber regions are most valuable once multi-fiber programs exist and frame/request patterns emerge.

---

## References

- Language spec: `docs/public/spec.md`
- RFC-0024: `docs/internal/rfcs/rfc-0024-linear-types.md` — `Region` is a linear type; all linear-type rules apply
- RFC-0001: `docs/internal/rfcs/rfc-0001-pointer-syntax.md` — pointer-into-region lifetime problem; `&x` restriction on linear values
- RFC-0003: `docs/internal/rfcs/rfc-0003-concurrency-model.md` — `Region` is not `Send`
- RFC-0006: `docs/internal/rfcs/rfc-0006-closure-capture-semantics.md` — move capture of region handles
- RFC-0026: `docs/internal/rfcs/rfc-0026-unsafe-blocks.md` — Option B (direct allocation) requires unsafe context
- Cluster report: `docs/internal/rfc-cluster-memory-model.md`
- Prior art: Cyclone regions, Rust arenas (`bumpalo` crate), Zig's `std.mem.Allocator`
