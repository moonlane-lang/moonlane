---
id: vision
title: "Project Vision"
type: guide
created_date: '2026-05-24'
---

# Project Vision

This document is the north star for the language's design and development direction. When a feature decision, RFC, or implementation choice is ambiguous, the answer should follow from this document. Everything here is intentional and has been reasoned through — treat departures from it as requiring explicit justification.

---

## Identity Statement

Moonlane is a statically typed, expression-oriented language with a Rust-inspired syntax that runs in two first-class modes: a production-quality interpreter and a native compiler. Both modes are maintained to the same standard — neither is a prototype, a stepping stone, or an afterthought. The same source file runs in both.

---

## The Competitive Position

The following combination of properties is unoccupied in the current language landscape. Properties marked *(planned)* are not yet implemented but are committed design targets — the claim is about the target state, not the current implementation.

- **Rust-like syntax** — algebraic types, pattern matching, expression orientation, no null, explicit error handling
- **First-class interpreter** — embeddable, scriptable, fast startup, REPL, no compile step required
- **First-class compiler** *(planned)* — native code, zero-cost linear types, no GC overhead when you need performance
- **Static type system in both modes** — the same type checker runs before execution whether you interpret or compile *(currently interpreter only; compiler will share the same frontend)*
- **Opt-in memory control** *(planned, RFC-0024)* — safe RC-managed memory by default; linear types for deterministic, zero-overhead allocation when needed

The closest reference point is OCaml: a language that commits to both a bytecode interpreter and a native compiler with equal engineering investment. Moonlane occupies an analogous position but in the Rust-influenced, systems-adjacent design space that OCaml does not target.

This is not "Rust but easier." Rust's ownership model solves a specific problem (fearless concurrency + zero-cost memory) with a specific mechanism (mandatory borrow checking). Moonlane solves a different problem: **expressive, safe code that is both scriptable and compilable without changing the source.** The mechanism is different — opt-in linear types instead of mandatory ownership — and the goals are different — dual-mode execution instead of systems programming exclusively.

---

## What "First-Class Both" Means

Committing to both modes is not a marketing claim. It has concrete engineering requirements:

### 1. A shared formal specification

The language spec (`docs/public/spec.md`) is the contract both backends must satisfy. Any behaviour not described in the spec is a bug in whichever backend exhibits it. The spec must be precise enough that it can be implemented twice, independently, and produce identical observable behaviour.

Implication: spec prose that is ambiguous enough to be implemented differently by the two backends is a spec bug, not an implementation choice.

### 2. A shared test suite

A cross-backend test corpus runs every Moonlane program against both the interpreter and the compiler and asserts identical output. Divergence between backends is a P0 bug regardless of which backend is "right." The test suite is the executable form of the spec.

Implication: any feature that cannot be tested in both modes is not shippable until both modes support it.

### 3. A feature parity policy

Every language feature must be available in both modes unless explicitly designated compiler-only or interpreter-only with a documented rationale. The default assumption is parity.

Designated exceptions (to be specified per feature):
- **Compiler-only**: features that are meaningless without code generation (e.g. `@inline` hints, link-time attributes)
- **Interpreter-only**: features that are meaningless without a live runtime (e.g. REPL-specific introspection)
- **Semantically shared, performance-different**: linear types are checked in both modes; zero-cost allocation only manifests in the compiler. This is not a parity violation — the behaviour is identical, the performance characteristic differs.

### 4. The interpreter is a product, not a prototype

The tree-walk interpreter is the first implementation of the language and remains a permanent, supported execution mode. It is not a stepping stone to be discarded when the compiler exists. Design decisions must not assume the interpreter will be replaced.

Concretely: the interpreter must be embeddable as a library (for scripting use cases), have a REPL, produce good error messages, and have a stable public API. These are product requirements, not nice-to-haves.

---

## Design Principle: Justify Features in Both Modes

Every language feature must answer the question: **what does this give the programmer in each mode?**

If a feature is only valuable in the compiler, it may still belong in the language — but its interpreter story must be documented. If it cannot be implemented in the interpreter without semantic compromise, that is an explicit design decision, not an oversight.

Examples:
- **Linear types**: interpreter gives static safety (resource leak detection at check time); compiler gives static safety plus zero-cost allocation. Both are genuine value. ✓
- **Unsafe blocks**: interpreter relaxes static checks (useful for FFI shims, region Option B); compiler additionally enables pointer arithmetic and raw memory. Semantically shared, capability-different. ✓
- **`@inline` hint**: meaningless in the interpreter (no code generation). Compiler-only by designation. The interpreter silently ignores it. ✓
- **Region allocation (Option A / scope-callback)**: interpreter allocates from a contiguous block and frees it on scope exit — same semantics, no performance benefit. Still valid as a memory organisation tool. ✓

---

## The Linear Types Story in Dual-Mode Context

Linear types are the feature whose value proposition is strongest in the dual-mode framing. In a compiler-only language, they are a memory management tool. In an interpreter-only language, they are a correctness tool with no performance payoff. In a dual-mode language, they are both — and the programmer gets to decide which they care about.

The value by mode:

| Mode | What linear types give you |
|---|---|
| Interpreter | Compile-time detection of resource leaks, double-frees, unconsumed handles — without any runtime overhead |
| Compiler | All of the above, plus zero-cost deterministic deallocation — no RC, no GC, no allocator overhead |

This is the honest story to tell programmers: *use the interpreter for scripting and rapid iteration; the linear type checker catches your resource management bugs either way; compile when performance matters and the zero-cost story kicks in.*

---

## Academic Research Angles

The dual-mode commitment, combined with the linear type system design, opens specific research questions with genuine publication potential. Three claims are defensible:

1. **Formal soundness of `&T` without lifetimes** — the expression-only read reference is a novel mechanism for inspection without consumption that avoids lifetime annotations. Provably sound? Under what conditions does it fail? This is pursuable with the interpreter alone.
2. **RC/linear coexistence formal model** — the boundary rules between RC-managed and linearly-managed values (including the `Send` interaction and channel-as-consumption semantics) have not been formally analyzed in the literature.
3. **Dual-mode semantic equivalence** — proving the interpreter and compiler implement identical semantics for the linear type system. Requires both backends; long-term.

The most achievable near-term contribution is formalizing the linear type system (Claim 1) — the inference rules are already described in prose in RFC-0024 and translating them to formal notation is the primary work.

See `docs/internal/research.md` for the full analysis: prior work map, per-claim prerequisites, explicit non-claims, and a publication strategy.

---

## What This Vision Does Not Mean

**It does not mean feature parity at all times.** The interpreter will have features first (it is easier to implement). The compiler will have features that the interpreter cannot express (e.g. link-time optimizations). Parity is the goal for stable features, not a release gate for every version.

**It does not mean equal performance.** The interpreter will always be slower than the compiler for CPU-bound workloads. This is expected and not a failure. They have different performance profiles for different use cases.

**It does not mean the language is trying to be everything.** Moonlane is not a general-purpose scripting language in competition with Python. It is not a systems language in competition with Rust or Zig. It is a language for programmers who want Rust-like expressiveness and safety in both a scriptable and a compilable form — a specific and narrow target.

**It does not mean the aesthetic is secondary.** The dark-fantasy visual identity, the wind-themed keywords, the deliberate naming choices — these are part of the project's personality. They are not in tension with the technical vision; they make the project recognisable.

---

## References

- Language spec: `docs/public/spec.md`
- Versioning model: `docs/internal/versioning.md`
- Memory model RFC cluster: `docs/internal/rfc-cluster-memory-model.md`
- RFC-0024: `docs/internal/rfcs/rfc-0024-linear-types.md`
- RFC-0025: `docs/internal/rfcs/rfc-0025-region-allocation.md`
- RFC-0026: `docs/internal/rfcs/rfc-0026-unsafe-blocks.md`
