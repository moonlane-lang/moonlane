# ADR-0012: Generic Struct Split-Env in Construction Pass

**Status:** Accepted  
**Date:** 2026-05-25  
**Issue:** #8

---

## Context

`build_concrete_struct_env` converts the `InferType`-keyed struct registry into
`Type`-keyed concrete fields by applying the final substitution. For non-generic
structs this is correct: the substitution resolves all field types globally.

For generic structs (e.g. `struct Box<T>`), field types contain TypeVars that are
**per-instantiation**, not global. The substitution only knows about TypeVars from
specific call sites; trying to apply it globally would either leave vars unresolved or
incorrectly smear one call site's bindings across another.

## Decision

**Two-env approach in `ConstructCtx`:**

1. `struct_scopes` (existing): `HashMap<String, Vec<(String, Type)>>` — concrete
   fields for **non-generic** structs only. `build_concrete_struct_env` filters out
   any struct with a `struct_type_params` entry.

2. `generic_struct_raw` + `generic_struct_type_params` (new): raw `InferType` fields
   and ordered type-param `TypeVar`s for **generic** structs.

At `FieldAccess` and `StructLiteral` construction sites:
- Non-generic struct → look up in `struct_scopes` (fast, pre-resolved).
- Generic struct → look up in `generic_struct_raw`, build a `Substitution` mapping
  `type_params[i] → type_args[i]` (extracted from the object's `Type::Named` args),
  apply it to the raw field `InferType`, convert to `Type`.

## Consequences

- **Correct per-instantiation types**: `Box<Int>.value : Int`, `Box<Bool>.value : Bool`.
- **No global smearing**: generic struct field types are not resolved until the
  concrete type args at the use site are known.
- **Two-branch dispatch**: every field access must check `generic_struct_type_params`
  first. This is O(1) (HashMap lookup) and unavoidable given the two-env design.
- **Invariant**: `generic_struct_raw` and `generic_struct_type_params` are always
  populated together by `build_registry`. A struct absent from `type_params` must
  also be absent from `raw`; a struct present in `type_params` must be present in
  `raw`. Both maps are read-only after construction.
