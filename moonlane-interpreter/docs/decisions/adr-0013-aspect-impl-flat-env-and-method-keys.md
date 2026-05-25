---
id: decision-13
title: "Aspect Impl Storage in Flat Environment with Structured String Keys"
date: '2026-05-25'
status: accepted
---

## Context

Sprint 6 adds aspect declarations, `impl Aspect for Type` blocks, and method dispatch via `.method()` syntax. Impl methods must be stored so the evaluator can look them up at method-call sites.

The evaluator already uses a flat `Environment: Vec<HashMap<String, Value>>` for all bindings. Two design questions arose:

1. Where do impl methods live — in the existing environment, or in a separate impl registry?
2. What key format uniquely identifies a method when multiple types or aspects share the same method name?

## Options Considered — Storage

### Option A: Separate `HashMap<(type_name, method_name), Value>` impl registry

A dedicated registry keyed by `(type, method)` pairs. The evaluator looks up method calls there instead of in the main environment.

**Pros:**
- No key collisions between different types' methods
- Clear separation between variable bindings and method bindings

**Cons:**
- Two lookups for method calls (check impl registry, then fall back to env)
- Construction pass already emits `TypedExpr::MethodCall` vs `TypedExpr::Call` — the evaluator knows statically which lookup to use, so a separate registry adds code without solving a real problem at this stage
- Does not interact well with closures capturing impl methods (closures capture the env, not a separate registry)

### Option B: Flat environment with structured string keys

Store impl methods in the existing environment using `"TypeName::method_name"` keys. The evaluator's `eval_method_call` constructs the key and calls `env.get()` exactly as it would for a variable.

**Pros:**
- Zero changes to environment lookup infrastructure
- Impl methods are captured by closures automatically
- Compatible with the existing Pass 1a/1b mutual-recursion tie-knot pattern
- The correct fix (Option A with a proper registry keyed by `(aspect, type_args, target)`) is isolated to the key format; the storage location can be changed independently

**Cons:**
- Key uniqueness depends entirely on the naming convention being followed consistently
- Two impl blocks for different types that happen to have the same method name collide only if the key prefix is missing — this is prevented by always prefixing with `TypeName::`
- Multiple `impl From<X> for Y` blocks for the same `Y` but different `X` would collide under `"Y::from"` (see Key Format section)

## Options Considered — Key Format

### Option A: `"TypeName::method_name"` for all impl methods

Simple. Works for all aspects except `From` when a type has multiple `From` impls.

### Option B: `"TypeName::AspectName<TypeArgs>::method_name"` for generic aspects

Encode the aspect name and type arguments in the key so each `impl From<IoError> for AppError` and `impl From<ParseError> for AppError` get distinct keys.

### Option C: Restrict structured key to `From` only

`From` is the only standard aspect where a single type legitimately has multiple impls (one per source type). Use the structured key only for `From`; all other aspects keep the simple `TypeName::method_name` key.

**Rationale:** Aspect bounds (#2) and object-safe dispatch (RFC-0008) are future work. Today, every other aspect allows at most one impl per type. Restricting the structured key to `From` keeps the common case simple and the special case explicit.

## Decision

**Option B (flat environment) with Option C (structured key only for From).**

The key format is:
- `"TypeName::method_name"` — all impl methods except From
- `"TypeName::From<SourceType>::from"` — `impl From<S> for T` methods

`impl_method_key()` in `evaluator/mod.rs` encodes this rule. The cast evaluator tries `"T::From<S>::from"` first, then falls back to `"T::from"` for compatibility.

## Consequences

- A future sprint must replace this with a proper `HashMap<(aspect_name, Vec<Type>, target_type), HashMap<method_name, Value>>` impl registry. Tracked as #133.
- All new aspect impls in the evaluator **must** use `impl_method_key()` — never hard-code the key string at call sites.
- The typechecker construction pass must mirror the evaluator's key for `PropagateError`: it emits `from_key = "Target::From<Source>::from"` at construction time so the evaluator can do a single `env.get(from_key)` lookup without re-deriving the key.

## References

- #130 — Aspect system implementation
- #12 — From aspect and `as` cast upgrade
- #133 — Tech debt: replace flat string-keyed impl env with proper impl registry
- [ADR-0006](adr-0006-evaluator-runtime-design.md) — Evaluator runtime design
