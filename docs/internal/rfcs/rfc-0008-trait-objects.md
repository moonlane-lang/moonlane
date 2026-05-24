---
id: rfc-0008
title: "Trait Objects (dyn Trait)"
date: '2026-05-21'
status: draft
---

## Summary

Add runtime polymorphism via trait objects: values whose concrete type is erased at compile time and dispatch happens through a vtable. The open question is the full semantics — fat-pointer representation, sizing, lifetime, and syntax.

---

## Motivation

The v0.2 trait system uses only static dispatch (generics + monomorphization). This is sufficient for most cases but requires the concrete type to be known at the call site. Trait objects enable:

- Heterogeneous collections (`Shape[]` holding `Circle` and `Rectangle`)
- Functions that return an opaque type without generic parameters at the call site
- Plugin-style APIs where the set of types is open

The spec ([Static Dispatch Only](../../public/spec/declarations.md#static-dispatch-only)) explicitly defers `dyn Trait` to a future version.

---

## Open Questions

- **Syntax**: `dyn Trait` (Rust-style), just `Trait` as a type (Go-style), or something else?
- **Sizing**: trait objects are unsized. Does Moonlane need a `Box<dyn Trait>` / heap-allocated wrapper, or does the runtime's RC model absorb this?
- **Object safety**: which traits can be used as trait objects? Methods with `Self` in position other than receiver break object safety in Rust. Does Moonlane adopt the same rule?
- **Interaction with generics**: can a generic function accept `dyn Trait` as a type argument?

---

## Decision

**Outcome:** *(pending)*  
**Target:** *(blank until accepted)*

*(Decision rationale goes here when the RFC is evaluated.)*
