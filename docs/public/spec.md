---
id: doc-2
title: "Moonlane Language Specification"
type: spec
version: v0.2
created_date: '2026-05-16'
---

> **Status:** Active. This document is the single source of truth for the Moonlane language.
> Features not described here are not part of the language.

Source files use the \`.mln\` extension.

---

## Overview

Moonlane is a statically typed, expression-oriented language with a Rust-inspired syntax that runs in two first-class execution modes: a production-quality interpreter and a native compiler. The same source file runs in both. Neither mode is a prototype or a stepping stone — both are permanent, supported, and maintained to the same standard.

The language's core design principles are:

- **Strong static typing** with full Hindley-Milner type inference
- **No classes** — data and behaviour are defined separately via structs, enums, and traits
- **Algebraic data types** — enums with data-carrying variants and exhaustive pattern matching
- **Explicit nullability** — absence of a value is represented by `Perhaps<T>`, never by null
- **Explicit error handling** — errors are values, represented as `Result<T, E>`
- **Safe memory by default** — reference counting, no ownership semantics required
- **Opt-in memory control** — linear types for deterministic, zero-overhead allocation in the compiler; static resource safety in the interpreter

### Execution modes

| Mode | Use case | Memory model |
|---|---|---|
| **Interpreter** | Scripting, embedding, rapid iteration, REPL | RC runtime; linear types enforced statically |
| **Compiler** | Production, performance-critical code, native binaries | Linear types enforced statically + zero-cost at runtime |

The type checker — including the linear type system — runs identically in both modes. Observable behaviour is identical. Performance characteristics differ: the compiler eliminates RC overhead for linear values; the interpreter does not.

See `docs/internal/vision.md` for the full design rationale and competitive positioning.

---

## Contents

| File | Contents |
|---|---|
| [Lexical Structure](spec/lexical.md) | Comments, identifiers, keywords, literals, operators |
| [Type System](spec/types.md) | Primitive types, inference, tuples, arrays, casting, generics, Never, `Perhaps<T>`, `Result<T,E>` |
| [Declarations](spec/declarations.md) | Variables, structs, enums, traits |
| [Functions](spec/functions.md) | Functions, closures, the `?` operator |
| [Expressions](spec/expressions.md) | Pattern matching, control flow |
| [Runtime](spec/runtime.md) | Panics, built-in functions |
| [Grammar](spec/grammar.md) | Formal grammar |

See [Changelog](../changelog.md) for version history.
