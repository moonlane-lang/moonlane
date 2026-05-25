<p align="center">
  <img src="media/moonlane-logo.svg" alt="Moonlane" width="600"/>
</p>

A statically typed, expression-oriented language that runs in two first-class modes: a production-quality interpreter and a native compiler. The same source file, the same type checker, both targets permanently supported.

## Why?

Most languages commit to one execution model. Scripting languages are interpreted and sacrifice performance. Systems languages compile and sacrifice startup time, embeddability, and the REPL. Moonlane is an attempt to refuse that trade.

The closest reference point is OCaml — a language with equal engineering investment in its bytecode interpreter and native compiler. Moonlane tries to occupy an analogous position in the Rust-influenced design space: **Rust-like expressiveness and safety, in both a scriptable and a compilable form, without the mandatory borrow checker.**

The mechanism is opt-in linear types instead of mandatory ownership. The goal is that the interpreter catches your resource management bugs at type-check time, and the compiler eliminates the runtime overhead on top of that — genuine value in both modes, not just one. Whether that holds up in practice is what this project is trying to find out.

## What?

### Available now (v0.2)

- **Strong static typing** with local type inference (Hindley-Milner)
- **Algebraic data types** — structs and enums with data-carrying variants
- **Exhaustive pattern matching**
- **Explicit nullability** via `Perhaps<T>` (no null pointers)
- **Explicit error handling** via `Result<T, E>` with `?` propagation
- **First-class functions** and closures
- **Generics** with trait bounds
- **Traits** for ad-hoc polymorphism
- **Runtime memory management** via reference counting

### Planned

- **Opt-in linear types** — the `linear` keyword marks a type as use-exactly-once. The type checker statically prevents resource leaks, double-frees, and unconsumed handles. No runtime overhead; in the compiler, linear values are freed deterministically with zero allocator cost.

- **Fiber green threads** — lightweight concurrent tasks launched with `spawn { }`. M:N scheduled by the runtime; no `async`/`await`, no function colouring. A function that blocks inside a fiber does not need a different declaration.

- **Typed channels** — `Chan<T>` is the primary concurrency primitive. Values are transferred between fibers with `ch <- value` (send) and `<- ch` (receive). A `select` expression waits on multiple channels simultaneously. Channels are the natural transport for linear values: sending consumes the value, satisfying the exactly-once rule across fiber boundaries.

- **C FFI** — `extern "C"` blocks declare functions callable via the C ABI. Calls require an `unsafe` block. The primary use case is Rust crate interop: any Rust crate can be exposed to Moonlane through a thin `#[no_mangle] extern "C"` shim, giving access to the full `crates.io` ecosystem.

See the [Language Specification](docs/public/spec.md) and [RFCs](docs/internal/rfcs/) for the complete design.

## How?

The spec and the interpreter are developed in parallel, in a tight loop:

```
Define a feature in the spec
        ↓
Implement it in the interpreter
        ↓
Write real programs using it
        ↓
Observe gaps, wrong assumptions, usability issues
        ↓
Refine the spec  →  implement the refinement  →  next feature
```

The spec is the contract both the interpreter and the future compiler must satisfy. Any behaviour not described in the spec is a bug in whichever backend exhibits it. The interpreter is not scaffolding to be discarded when the compiler arrives — it is a permanent, supported execution mode with its own product requirements: embeddable as a library, a REPL, good error messages, stable public API.

## Quick Start

### Prerequisites

- Rust 1.70+
- Cargo

### Build

```bash
cd moonlane-interpreter
cargo build --release
```

### Run a Program

```bash
cargo run -- path/to/program.mln
```

### Run Tests

```bash
# All tests
cargo test

# Type inference unit tests
cargo test --test typeinference_tests

# Typechecking integration tests
cargo test --test typechecking_tests
```

## Example

```moonlane
fun factorial(n: Int) -> Int {
    if (n <= 1) { 1 } else { n * factorial(n - 1) }
}

let result = factorial(5);
```

## Project Structure

```
Moonlane/
├── moonlane-interpreter/
│   ├── src/
│   │   ├── parser/         # PEG grammar (pest) → untyped AST
│   │   ├── ast/            # Untyped AST node definitions
│   │   ├── typeinference/  # HM inference engine
│   │   ├── typechecker/    # Two-pass type checker → typed AST
│   │   ├── typed_ast/      # Typed AST node definitions
│   │   ├── evaluator/      # Tree-walking evaluator
│   │   ├── types/          # Concrete type representation
│   │   └── error/          # Unified error type
│   ├── tests/
│   │   ├── typeinference/  # HM engine unit tests (phases 1–7)
│   │   ├── typechecking/   # Full pipeline integration tests
│   │   └── parsing/        # Parser tests
│   └── Cargo.toml
│
└── docs/           # Spec, RFCs, Changelog
```

## Resources

- **Language Specification:** [`docs/public/spec.md`](docs/public/spec.md)
- **Project Vision:** [`docs/internal/vision.md`](docs/internal/vision.md)
- **Typechecker Architecture:** [`moonlane-interpreter/docs/typechecker.md`](moonlane-interpreter/docs/typechecker.md)
- **Evaluator Design:** [`moonlane-interpreter/docs/evaluator.md`](moonlane-interpreter/docs/evaluator.md)
- **RFCs:** [`docs/internal/rfcs/`](docs/internal/rfcs/) — language change proposals and decisions
- **Decision Records:** [`moonlane-interpreter/docs/decisions/`](moonlane-interpreter/docs/decisions/) — implementation rationales

## License

Moonlane is licensed under the Apache License 2.0. See the [LICENSE](LICENSE) file for details.
