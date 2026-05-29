---
id: ADR-0025
title: Unified TypeDefinitionRegistry shared across inference and construction passes
status: accepted
date: 2026-05-29
issues: ["#133"]
---

## Context

Before Sprint 12, the typechecker maintained type and impl data in two separate locations:

- **Pass 1 (inference):** `TypeRegistry` struct owned by `InferContext`, holding `struct_env`, `method_env`, `enum_env`, `aspect_env`, and `impl_aspect_env` as `InferType`-valued maps.
- **Pass 2 (construction):** `ConstructCtx` received four separate `HashMap` arguments (`concrete_struct_env`, `concrete_method_env`, `enum_env`, `impl_aspect_env`) built by calling methods on the inference-time registry. These were independent copies.

This split meant:
- `ConstructCtx::new` had 8 arguments (4 were these map copies)
- Adding a new type-data field required changes in at least 3 places
- Build functions (`build_concrete_struct_env`, `build_concrete_method_env`) were on the inference registry, creating a coupling risk

## Decision

Introduce a single `TypeDefinitionRegistry` that is:
- Owned by `InferContext` (populated during pre-pass, used throughout Pass 1)
- Borrowed by `ConstructCtx` as `&TypeDefinitionRegistry` — no copy, no rebuild
- The source of truth for all struct, enum, method, aspect, and impl-aspect data in both passes

`ConstructCtx::new` takes 4 arguments (down from 8). The concrete env builders (`build_concrete_struct_env`, `build_concrete_method_env`) are free functions in `construction.rs` that borrow from the registry and apply the substitution.

`FieldEntry` is a type alias for `(String, InferType, Span)` — fields now carry their declaration span so Pass 2 can report accurate error locations.

## Alternatives Considered

**Keep separate maps for construction pass** — avoids the lifetime constraint (`ConstructCtx` holding `&TypeDefinitionRegistry`), but keeps the duplicated-rebuild pattern and means any new field needs to thread through 4 call sites.

**Materialise concrete maps at registry construction** — would require running substitution before Pass 1 completes, which is impossible (substitution is only complete after Pass 1 constraint solving).

## Consequences

- Single upgrade point: adding a new kind of type data requires one field on `TypeDefinitionRegistry`
- `ConstructCtx` holds a `'a`-lifetime reference into `InferContext`'s registry; they must not alias mutably — this is enforced by the borrow checker since `ConstructCtx` is only created after `InferContext`'s pre-pass completes
- ADR-0001's intent (single authoritative registry) is now realised; the prior retroactive review noted the flat-map design failed to achieve it
