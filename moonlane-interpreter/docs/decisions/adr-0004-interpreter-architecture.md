---
id: decision-4
title: "Interpreter Architecture"
date: '2026-04-04'
status: accepted
---
## Context

We are building the first Moonlane interpreter. Its primary goal is spec validation — proving that the v0.1 language spec is complete, consistent, and implementable. Performance is a secondary concern at this stage.

Three architectural decisions needed to be made before writing any code.

---

## Decision 1 — Tree-walking interpreter over bytecode VM

### Options Considered

**Option A: Tree-walking interpreter** — evaluate the AST directly by recursive tree traversal. No intermediate representation.

**Option B: Bytecode VM** — compile the AST to a bytecode instruction set, execute in a virtual machine.

### Decision

**Chosen: Option A — tree-walking interpreter.**

The goal of v0.1 is spec validation, not performance. A tree-walker can be built and iterated on much faster, and is significantly easier to debug when spec ambiguities surface (the stack trace maps directly to language constructs). A bytecode VM can be introduced as an intermediate step between the interpreter and the LLVM compiler if desired — but that is a future decision. A tree-walker does not foreclose that path.

---

## Decision 2 — Static type checker as a separate pre-evaluation pass

### Options Considered

**Option A: Static type checker pass** — after parsing, run a dedicated type-checking pass over the AST before evaluation begins. Type errors are caught before any code runs.

**Option B: Runtime type errors only** — skip a static type checker; type mismatches surface as runtime panics during evaluation.

### Decision

**Chosen: Option A — static type checker pass.**

The spec defines Moonlane as statically typed. Implementing only runtime type checking would validate the runtime behaviour of the interpreter but not the type system itself — which is one of the most complex and most important parts of the spec (inference, generics, trait bounds, `Perhaps<T>`, `Result<T,E>`). A static type checker validates those spec sections directly. It is also a prerequisite for the LLVM compiler, which will need a fully typed AST. Building it now avoids rebuilding it later.

The type checker runs as a distinct pass: `source → parse → AST → type check → (typed AST) → evaluate`.

---

## Decision 3 — Monomorphisation for generics

### Options Considered

**Option A: Monomorphisation at call sites** — for each use of a generic function or type with concrete type arguments, instantiate a specialised concrete version at interpretation time. This mirrors what a compiler would do.

**Option B: Runtime type tags** — keep generic code polymorphic at runtime; use tagged/boxed values to carry type information dynamically.

### Decision

**Chosen: Option A — monomorphisation.**

Consistent with the eventual LLVM compiler architecture. Builds the right mental model for how generics work in Moonlane. Type tags would diverge from the compiler model and make the transition harder. Runtime overhead of monomorphisation is acceptable in an interpreter.

---

## Consequences

- The pipeline is: `source → lexer/parser (pest) → CST → AST builder → type checker → tree-walking evaluator`
- The type checker produces a typed AST; the evaluator consumes it
- Generics are resolved during type checking / at call sites; the evaluator sees only monomorphised types
- Performance is not a design constraint for v0.1

## References

- Architecture: [../architecture.md](../architecture.md)
- Spec: [docs/public/spec.md](../../../docs/public/spec.md)
