---
id: rfc-0027
title: "C FFI"
date: '2026-05-24'
status: draft
target:
---

## Summary

Add a C foreign function interface (FFI) that lets Moonlane programs call functions exported via the C ABI. Moonlane declares external functions with `extern "C"` blocks; calling them requires an `unsafe` block. The build tool provides first-class support for Rust crates as the primary use case — any Rust crate can be exposed to Moonlane by writing a thin `#[no_mangle] extern "C"` shim. Plain C libraries are supported identically. A future RFC will cover auto-binding generation for safe Rust APIs.

---

## Motivation

Moonlane targets programmers who already think in Rust idioms. A recurring objection to adopting any new language is "but I'd have to rewrite all my libraries." Rust's ecosystem (`crates.io`, ~160k crates as of 2026) is one of the strongest in systems programming. Without a path to reuse it, Moonlane forces users to choose between the language they want and the libraries they need.

The specific use cases that motivate this:

- **Serialization**: `serde` + format crates (`serde_json`, `serde_msgpack`, etc.) are industry standard and have no peer in quality or performance.
- **Networking and async I/O**: `tokio`, `reqwest`, `hyper` — building equivalents from scratch is multi-year work.
- **Cryptography**: `ring`, `rustls`, `sha2` — security-critical libraries where reimplementation is a liability.
- **Parsing**: `nom`, `pest`, `logos` — mature, battle-tested combinator and lexer frameworks.
- **Database**: `sqlx`, `rusqlite` — typed, async-capable DB clients.

Python's trajectory is instructive: the ability to call into C extensions (NumPy, SciPy, PIL) transformed Python from a scripting curiosity into serious infrastructure. Moonlane's Rust-syntax users have an even stronger pull toward the Rust ecosystem than Python users had toward C.

This RFC scopes to **Phase 1: C ABI interop**. Rust can expose any API surface via `#[no_mangle] extern "C"` functions, and any Rust crate can be wrapped in a thin binding shim. This gives disproportionate coverage for moderate effort and establishes the FFI machinery that later phases build on.

---

## Proposal

### 1. `extern "C"` blocks

An `extern "C"` block declares functions that are implemented externally and callable via the C ABI. Declarations are signatures only — no body.

```moonlane
extern "C" {
    fn moonlane_json_serialize(ptr: OpaquePtr, len: Int) -> MoonlaneStr;
    fn moonlane_json_free(ptr: OpaquePtr);
    fn moonlane_crypto_sha256(input: OpaquePtr, len: Int, out: OpaquePtr);
}
```

`extern "C"` blocks may appear at the module top level. All declared names are in scope within the module and may be re-exported.

### 2. Calling extern functions requires `unsafe`

Extern function calls may violate Moonlane's memory and type safety invariants — the callee is not checked by the Moonlane type system. Calls are therefore only permitted inside `unsafe` blocks (see RFC-0026):

```moonlane
fn serialize(value: SomeType) -> String {
    unsafe {
        let raw = value.as_opaque_ptr();
        let result = moonlane_json_serialize(raw, value.byte_len());
        result.into_string()
    }
}
```

Attempting to call an extern function outside `unsafe` is a type error.

### 3. C-compatible types

The following types are valid in `extern "C"` signatures. They have a defined C ABI representation and may cross the FFI boundary:

| Moonlane type | C type | Notes |
|---|---|---|
| `Int` | `int64_t` | Always 64-bit |
| `Float` | `double` | 64-bit |
| `Bool` | `uint8_t` | 0 or 1 |
| `OpaquePtr` | `void*` | Untyped pointer; value is opaque to Moonlane |
| `MoonlaneStr` | `struct { char* ptr; size_t len; }` | Non-owning view; see §3.1 |
| `()` | `void` | Return type only |

No other types are permitted in `extern "C"` signatures. Using a non-C-compatible type in an extern declaration is a compile-time error.

#### 3.1 `MoonlaneStr` and ownership

`MoonlaneStr` is a non-owning, non-null-terminated byte slice. When a Rust binding returns a `MoonlaneStr`, the binding is responsible for lifetime management — Moonlane does not free it. The binding shim is expected to either:

- Return a view into a static or arena-allocated buffer, or
- Require the caller to pass a free function (see below).

This is intentionally conservative. A higher-level string-owning type that triggers a free callback on drop may be added in a follow-up RFC once the linear type system (RFC-0024) is available to enforce the single-free invariant.

### 4. Linking and build integration

A new `[rust-bindings]` section in `moonlane.toml` specifies Rust crate dependencies:

```toml
[rust-bindings]
my_moonlane_bindings = { path = "./bindings/my_moonlane_bindings" }
```

Each entry is a path to a Rust crate that exposes a `extern "C"` surface. The Moonlane build tool:

1. Runs `cargo build --release` in the specified crate directory.
2. Locates the produced `.so` / `.dll` / `.a` in the crate's `target/` directory.
3. Links it into the Moonlane program (static for the compiler, dynamic for the interpreter via `dlopen`).

Remote crate paths (e.g. `crates.io` coordinates) are out of scope for this RFC. The expectation is that users write a thin binding shim crate locally, which declares `#[no_mangle] extern "C"` wrappers around whichever upstream crate they want to expose.

#### 4.1 Interpreter: dynamic loading via `libffi`

In the interpreter, extern functions are resolved at startup by `dlopen`ing the compiled `.so` and loading symbol addresses. Calls are dispatched at runtime via `libffi`, which handles ABI-correct argument marshalling without generating machine code.

The interpreter reports a startup error (not a runtime panic) if a required `.so` is missing or a declared symbol cannot be found.

#### 4.2 Compiler: standard C linker integration

In the compiler, `extern "C"` declarations emit standard LLVM `declare` stubs. The compiled Rust crate is passed to the linker as a static or dynamic library. No special machinery is required beyond what any C FFI compiler does.

### 5. Binding shim conventions

A binding shim is a normal Rust crate. No special framework is required. A minimal example wrapping a hypothetical `mycrate`:

```rust
// bindings/my_moonlane_bindings/src/lib.rs

use mycrate::serialize;

#[repr(C)]
pub struct MoonlaneStr {
    ptr: *const u8,
    len: usize,
}

#[no_mangle]
pub extern "C" fn moonlane_serialize(input: *const u8, len: usize, out_len: *mut usize) -> *const u8 {
    let slice = unsafe { std::slice::from_raw_parts(input, len) };
    let result = serialize(slice); // returns Vec<u8>
    let boxed = result.into_boxed_slice();
    unsafe { *out_len = boxed.len() };
    Box::into_raw(boxed) as *const u8
}

#[no_mangle]
pub extern "C" fn moonlane_serialize_free(ptr: *const u8, len: usize) {
    unsafe {
        let _ = Box::from_raw(std::slice::from_raw_parts_mut(ptr as *mut u8, len));
    }
}
```

The binding shim author is responsible for memory safety across the boundary. The Moonlane side wraps the calls in `unsafe` and manages lifetimes through Moonlane's linear type system (once RFC-0024 is available) or manually.

### 6. Panic safety

Rust panics must not unwind across the FFI boundary. Binding shims must use `std::panic::catch_unwind` at every `extern "C"` entry point and translate panics to error return codes or null pointers. Unwinding across a C boundary is undefined behaviour.

The Moonlane build tool will emit a warning (not an error) if it detects that a binding shim crate was compiled without `-C panic=abort` and does not use `catch_unwind` at its public surface. Full static enforcement is not feasible at this stage.

---

## Alternatives Considered

### A. Build a Moonlane standard library from scratch

The most isolation-preserving option. Every library is written in Moonlane, with no external dependencies. **Rejected** as a sole strategy because it is multi-year work and abandons the Rust ecosystem entirely. A standard library is desirable and should be pursued in parallel (see RFC-0016), but it cannot substitute for FFI in the near term.

### B. WASM as an interop layer

Compile Rust crates to WASM modules; host them in the Moonlane runtime. **Rejected** because: WASM introduces serialization overhead on every call (memory copy in/out of the WASM heap), prevents sharing memory with the host, and is incompatible with the zero-overhead story in compiler mode. It also makes the interpreter significantly more complex for no semantic benefit over C FFI.

### C. Direct Rust ABI interop (no shim required)

Parse Rust MIR or the crate's public API, auto-generate Moonlane bindings, and call using Rust's internal ABI. **Rejected for this RFC** because Rust's internal ABI is not stable and changes between compiler versions. Feasible as a future tool (moonlane-bindgen) that generates C shims automatically and emits both the Rust-side `extern "C"` wrappers and the Moonlane-side `extern "C"` declarations. This is tracked as a follow-on.

### D. C FFI only (no Rust integration)

Accept any `.h`-style declaration, link against `.so` / `.a` from any source. This is strictly more general than this RFC. **Considered** and intentionally narrowed: the build tool integration in §4 is Rust-specific because the dominant use case is `crates.io`. The underlying `extern "C"` mechanism is identical to generic C FFI, so supporting arbitrary C libraries is a trivial extension of this RFC and can be done by accepting a `path` pointing to a pre-built `.a`/`.so` rather than a Cargo crate.

---

## Open Questions

1. **`MoonlaneStr` ownership story**: the current proposal punts on owned string returns. Is a `MoonlaneOwnedStr` with an explicit free-function callback the right model before linear types are available, or should this wait for RFC-0024?

2. **Struct passing across FFI**: `#[repr(C)]` Rust structs can be passed by value if both sides agree on the layout. Should Moonlane support declaring `extern struct` types with explicit field layouts? This would allow richer APIs without opaque-pointer indirection.

3. **Build tool scope**: should `moonlane.toml` support `crates.io` coordinates directly (triggering a `cargo fetch`), or keep the scope at local paths only? Local paths require the user to vendor dependencies; remote coordinates create a Cargo wrapper.

4. **Auto-binding generation (moonlane-bindgen)**: a tool that reads a Rust crate's public API and generates both the shim `lib.rs` and the Moonlane `extern "C"` block would dramatically reduce friction. Should this be scoped into this RFC or tracked separately?

5. **`async` Rust**: `tokio`-based crates are among the most valuable in the ecosystem. Calling into async Rust from synchronous Moonlane requires a `tokio::Runtime::block_on` wrapper in the shim. Should the build tool or a standard binding helper emit this boilerplate, or is it always the shim author's responsibility?

6. **Platform ABI**: the type table in §3 assumes a 64-bit LP64 platform. The `Int`-to-`int64_t` mapping and struct layout assumptions need an explicit ABI section covering 32-bit targets, Windows (LLP64), and WASM32.

---

## Timing Recommendation

This RFC should not be implemented before:

1. **RFC-0026 (Unsafe Blocks)** is accepted and implemented — calling extern functions requires `unsafe`, which is not yet in the language.
2. **Generics (v0.2)** are complete — the `OpaquePtr` and `MoonlaneStr` types need to fit cleanly into the type system, and the design is cleaner with generics available.

The earliest practical target is **v0.3**. The build tool integration (§4) and interpreter-side `libffi` wiring are the bulk of the implementation work; the language-level changes (the `extern "C"` block syntax and the type checker additions) are modest.

---

## References

- RFC-0024: `docs/internal/rfcs/rfc-0024-linear-types.md`
- RFC-0025: `docs/internal/rfcs/rfc-0025-region-allocation.md`
- RFC-0026: `docs/internal/rfcs/rfc-0026-unsafe-blocks.md`
- RFC-0016: `docs/internal/rfcs/rfc-0016-standard-library.md`
- Language spec: `docs/public/spec.md`
- Rust reference, FFI: https://doc.rust-lang.org/reference/items/external-blocks.html
- `libffi`: https://sourceware.org/libffi/
