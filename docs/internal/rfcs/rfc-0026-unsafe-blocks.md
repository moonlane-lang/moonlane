---
id: rfc-0026
title: "Unsafe Blocks"
date: '2026-05-24'
status: draft
target:
---

## Summary

Introduce `unsafe { }` blocks and `unsafe fun` declarations as explicit escape hatches from the compiler's static safety guarantees. Inside an unsafe context, the linearity checker is relaxed, pointer arithmetic beyond RFC-0001's safe model is permitted, region-allocated values are accessible without a scope callback, and the `Send` marker can be bypassed. Type checking and syntax remain enforced. The `unsafe` keyword makes programmer-asserted correctness visible at the source level and auditable by tooling.

---

## Motivation

The safe memory model (RFC-0024 linear types, RFC-0001 pointers, RFC-0003 Send constraints) is statically verified. This verification is necessarily conservative: the compiler rejects programs it cannot prove safe, including some programs that are in fact correct but whose correctness depends on invariants the type system cannot represent.

Concrete cases that safe Gust cannot express:

- **FFI.** Calling a C function requires raw pointers, which have no safety guarantees at the boundary. The type system cannot verify that a `*mut T` passed to `malloc`/`free` is used correctly.
- **Hardware/OS interfaces.** Memory-mapped I/O, syscalls, and interrupt handlers operate at addresses the type system knows nothing about.
- **Custom allocators.** Implementing `Region` (RFC-0025) itself requires unsafe operations — the backing block is a raw allocation, and `create` places values at computed offsets.
- **Known-safe aliasing.** Some data structures (e.g. intrusive linked lists, certain lock-free queues) require aliased mutable access that the type system forbids but the programmer can verify is sequenced correctly.
- **Performance-critical region access.** RFC-0025's Option B (direct `region.create()` outside a scope callback) requires the programmer to assert that region-allocated values do not outlive the region.

Without an escape hatch, these use cases require the programmer to work around the type system in ways that are less readable and harder to audit. With `unsafe`, the unsafe surface is explicit, localized, and searchable.

---

## Proposal

### `unsafe { }` blocks

An `unsafe` block is an expression that opts its body out of a specific set of static checks. Outside those checks, the body is otherwise normal Gust code — types are still checked, syntax is still enforced.

```gust
let result = unsafe {
    let raw = region.create_unchecked(MyStruct { ... });
    raw.field + 1
};
```

The return type of `unsafe { ... }` is the type of the last expression in the block, same as any other block.

`unsafe` blocks may appear anywhere an expression is valid. They do not require a special function context — you can write an `unsafe` block in the middle of a safe function. However, calling an `unsafe fun` requires being inside an `unsafe` block.

### `unsafe fun`

A function declared `unsafe fun` may only be called from within an `unsafe { }` block or another `unsafe fun`. Calling an `unsafe fun` outside an unsafe context is a compile error.

```gust
unsafe fun memcpy(dst: *mut Byte, src: *const Byte, n: Int) {
    // raw memory copy — no bounds checking, no type safety
}

// call site:
unsafe {
    memcpy(dst_ptr, src_ptr, 64);
}
```

`unsafe fun` is how the standard library and FFI bindings expose operations that are correct under specific preconditions the caller must establish.

### What `unsafe` relaxes

| Check | Safe Gust | Inside `unsafe` |
|---|---|---|
| Linearity — double use | Compile error | Permitted |
| Linearity — unconsumed value | Compile error | Permitted |
| `*T`/`*mut T` to a linear value | Compile error (RFC-0001) | Permitted |
| Pointer arithmetic (`ptr.offset(n)`) | Not available | Available |
| `Region::create` outside scope callback | Not available (RFC-0025) | Available |
| `Send` marker bypass (cross-fiber without `Send`) | Compile error (RFC-0003) | Permitted via `unsafe_send(value)` |
| Calling `unsafe fun` | Compile error | Permitted |

### What `unsafe` does not relax

Type checking remains fully enforced inside `unsafe`. An expression of type `Int` cannot be assigned to a binding of type `String`. Pattern exhaustiveness is still checked. Syntax is unchanged. `unsafe` is not "anything goes" — it is a precisely scoped relaxation of the memory safety checks.

### `unsafe_send` — cross-fiber bypass

To pass a non-`Send` value across a fiber boundary (e.g. to implement `Arc` or a lock-free queue), an explicit `unsafe_send(value)` cast is required inside an `unsafe` block:

```gust
unsafe {
    let raw: *mut T = get_shared_ptr();
    ch <- unsafe_send(raw);   // programmer asserts Send is safe here
}
```

`unsafe_send` is a built-in that wraps a value in a `Send`-marked shell. It is only callable inside `unsafe`. The programmer asserts that cross-fiber access is correctly synchronized.

### `unsafe` and linear types

Inside an `unsafe` block, the linearity checker is relaxed — a linear value may be used zero times or more than once. This does not change the semantics at runtime — double-freeing or leaking a resource still causes the same runtime consequences. `unsafe` just means the programmer has asserted these won't happen.

```gust
let buf = Buffer::alloc(1024);
unsafe {
    let raw: *mut Byte = &mut buf as *mut Byte;  // take pointer to linear value — forbidden in safe code
    // buf is now accessible both through raw and through buf
    // programmer asserts only one path will free the backing memory
}
buf.free();
```

### Propagation

`unsafe` does not propagate. A safe function called from within an `unsafe` block is still subject to all safe-Gust rules. Only the direct body of the `unsafe { }` block or `unsafe fun` is relaxed.

The inverse also holds: an `unsafe fun` can call safe functions freely.

---

## Alternatives Considered

### No unsafe — rely solely on safe primitives

If linear types, regions, and the safe pointer model cover all practical use cases, `unsafe` is unnecessary. This was the initial position when linear types were chosen over unsafe blocks as the primary memory management mechanism.

**Why this is insufficient:** FFI cannot be expressed without some form of unsafe. The standard library itself (allocators, platform bindings) requires operations the type system cannot verify. A language without an escape hatch either cannot do FFI or silently marks its entire FFI layer as trusted without making that visible.

### `extern` blocks only (C FFI-scoped unsafe)

Restrict the escape hatch to `extern` declarations that call into C. Unsafe is only required at the FFI boundary; all Gust-to-Gust code is safe.

**Partial merit:** this is simpler and more restricted. However, it does not cover the region allocation (Option B), lock-free data structure, or custom allocator use cases. Those require unsafe operations entirely within Gust code.

### Trusted functions (no block syntax)

Instead of a block syntax, mark specific functions as `trusted` (analogous to `unsafe fun`). Calling a trusted function is always allowed; the trust annotation signals that the implementation requires care.

**Rejected:** this makes the unsafe surface invisible at call sites. With `unsafe { }`, every call to an `unsafe fun` is surrounded by a visible block. With trusted functions, the unsafe call looks like any other call. The block-level annotation is important for auditability — you can grep for `unsafe` and find every place the programmer has asserted correctness manually.

---

## Open Questions

1. **Unsafe types.** Should there be a way to declare a type that can only be constructed inside `unsafe`? This would be useful for raw pointer wrappers where the invariant is established at construction time (e.g. `NonNull<T>` in Rust). Without unsafe types, any function that returns a raw pointer is implicitly unsafe by convention rather than by the type system.

2. **`unsafe` in closures.** Can a closure body contain `unsafe { }` blocks? Can an `unsafe fun` be written as a closure literal? The natural answer is yes to both, but the interaction with RFC-0006's `send fun` qualifier needs consideration: is an `unsafe fun` closure `send`?

3. **Auditing and tooling.** Should the compiler emit a summary of unsafe surface area (count of `unsafe` blocks and `unsafe fun` declarations) as a build artifact? This would support security audits. Alternatively, a lint tool could provide this separately.

4. **`unsafe` and the FFI story.** This RFC does not define how FFI works (calling C, linking against external libraries). The `unsafe fun` declaration form is the right hook for FFI function signatures, but the full FFI design (name mangling, calling conventions, `extern "C"` equivalent) is out of scope here and should be a separate RFC.

5. **Unsafe surface minimisation.** Should the language or tooling enforce a maximum `unsafe` block size, or require that `unsafe` blocks be wrapped in a safe abstraction (i.e. `unsafe` cannot appear in `pub` function bodies — only in private helpers)? This would push the pattern of "small unsafe core, safe public API" as a convention rather than a recommendation.

---

## Timing Recommendation

`unsafe` depends on the safe memory model being stable. There is no point defining what the escape hatch escapes from before the safe layer is finalized. Target **v0.4** — after linear types (RFC-0024, v0.3), pointer syntax (RFC-0001, v0.3), and closure capture (RFC-0006, v0.3) are implemented. The FFI story can be deferred to v0.5 or later.

The minimum action before closing this RFC: the `unsafe fun` form must be agreed upon at the same time as RFC-0001's pointer syntax, since FFI-style pointer declarations will use `unsafe fun` signatures. Defer the block syntax implementation but lock in the `unsafe fun` syntax as part of v0.3.

---

## References

- Language spec: `docs/public/spec.md`
- RFC-0001: `docs/internal/rfcs/rfc-0001-pointer-syntax.md` — pointer arithmetic gated on unsafe; `*T` to linear values requires unsafe
- RFC-0003: `docs/internal/rfcs/rfc-0003-concurrency-model.md` — `unsafe_send` bypasses `Send` constraint
- RFC-0006: `docs/internal/rfcs/rfc-0006-closure-capture-semantics.md` — unsafe closures; `unsafe fun` as closure literal
- RFC-0024: `docs/internal/rfcs/rfc-0024-linear-types.md` — linearity checker relaxed inside unsafe
- RFC-0025: `docs/internal/rfcs/rfc-0025-region-allocation.md` — Option B (direct region allocation) requires unsafe
- Cluster report: `docs/internal/rfc-cluster-memory-model.md`
- Prior art: Rust `unsafe` keyword and reference, Zig `@ptrCast` / `@intToPtr`, C's implicit unsafety model
