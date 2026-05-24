---
id: decision-1
title: "TypeRegistry Structure and Location"
date: '2026-05-07'
status: accepted
---
## Context

Task 0004 of epic-005 (Stage 4) needs to type-check `StructLiteral`, `FieldAccess`, and
enum variant patterns in `match`. All three require knowing the field and variant
types of user-defined types at inference time — information that currently exists
only in the raw `StructDecl`/`EnumDecl` AST nodes and is not accessible during
the inference walk.

Two related questions must be answered together because the location affects the
construction sequence and the structure affects what the API looks like:

1. **Structure** — how is type definition information stored and queried?
2. **Location** — where does the registry live relative to `InferContext`?

Constraints:
- Generic structs/enums are stubbed in v0.1 (non-empty `generics` → error),
  so the structure only needs to handle monomorphic types for now.
- v0.3 will need to instantiate generic types at use sites — the structure
  should not require a full rewrite to support that.
- v0.3 will need to store trait implementations per type — should be addable
  without restructuring the registry.

## Options Considered — Structure

### Option A: `TypeDef` enum with typed lookup methods

```rust
pub struct StructInfo {
    pub fields: Vec<(String, InferType)>,
}

pub struct EnumInfo {
    pub variants: HashMap<String, Vec<(String, InferType)>>,
}

pub enum TypeDef {
    Struct(StructInfo),
    Enum(EnumInfo),
}

pub struct TypeRegistry {
    defs: HashMap<String, TypeDef>,
}

impl TypeRegistry {
    pub fn lookup(&self, name: &str) -> Option<&TypeDef>;
    pub fn field_type(&self, struct_name: &str, field: &str, span: &Span)
        -> Result<InferType, MoonlaneError>;
    pub fn variant_fields(&self, enum_name: &str, variant: &str, span: &Span)
        -> Result<&[(String, InferType)], MoonlaneError>;
}
```

Field types are converted from `TypeExpr` to `InferType` once, during the
pre-pass. Callers ask by name and get back a typed result or a located error.

**Pros:**
- Typed distinction between structs and enums — callers that expect a struct get
  a type error at the registry boundary if given an enum name, not a silent wrong
  answer deeper in inference
- Lookup methods encapsulate error production (span-aware), so call sites are clean
- v0.3 (generics) path is clear: change `Vec<(String, InferType)>` to
  `Vec<(String, TypeScheme)>` and make `field_type` instantiate with fresh vars;
  the method signature stays the same
- v0.3 (traits) path is clear: add `impls: Vec<ImplInfo>` to `StructInfo`/`EnumInfo`
  or keep a parallel `ImplTable` — either fits without restructuring

**Cons:**
- More upfront code than a flat map
- `TypeDef` enum adds a layer of indirection for callers that only need to know
  "does this name exist"

### Option B: Flat `HashMap<String, Vec<(String, InferType)>>`

Two separate maps, one for structs and one for enums:

```rust
pub struct TypeRegistry {
    structs: HashMap<String, Vec<(String, InferType)>>,
    enums:   HashMap<String, HashMap<String, Vec<(String, InferType)>>>,
}
```

**Pros:**
- Minimal code to write for v0.1

**Cons:**
- No typed distinction at the boundary — callers must know which map to look in,
  and a wrong lookup silently returns `None` instead of "this is an enum, not a
  struct"
- v0.3 (generics) migration touches the map value types directly everywhere; no single
  conversion point
- Harder to extend for traits (no natural `TypeDef` attachment point)

### Option C: Store raw `TypeExpr`, convert on demand

```rust
pub struct TypeRegistry {
    defs: HashMap<String, TypeDef>,  // TypeDef holds raw AST nodes
}
```

Field types stay as `TypeExpr` and are converted to `InferType` each time a
field is looked up.

**Pros:**
- No conversion cost at pre-pass time
- Naturally supports generics: substitute type params into `TypeExpr`, then convert

**Cons:**
- `TypeExpr → InferType` conversion must be available at lookup time, which means
  threading a converter or a reference to the conversion logic into every lookup call
- Repeated conversion of the same types on every access
- For v0.1 (no generics), the on-demand conversion buys nothing

## Options Considered — Location

### Option A: Field on `InferContext`

```rust
pub struct InferContext {
    var_gen:         TypeVarGenerator,
    mono_env:        Vec<HashMap<String, (InferType, bool)>>,
    poly_env:        HashMap<String, TypeScheme>,
    constraints:     Vec<Constraint>,
    type_registry:   TypeRegistry,   // ← added
}
```

The pre-pass populates `ctx.type_registry` before the inference walk begins.

**Pros:**
- Everything inference-related is in one place — no extra parameter threading
- Consistent with how `mono_env` and `poly_env` are already managed

**Cons:**
- `InferContext` grows; the registry is immutable after the pre-pass but lives
  in a mutably-borrowed struct for the entire inference walk
- Harder to test the registry in isolation

### Option B: Pre-built and injected

```rust
impl InferContext {
    pub fn new(type_registry: TypeRegistry) -> Self { ... }
}
```

The pre-pass builds a `TypeRegistry` independently, then `InferContext::new`
takes ownership of it.

**Pros:**
- Clear construction sequence: registry is fully built and immutable before
  inference starts; no risk of a half-populated registry being queried mid-walk
- Registry can be unit-tested without constructing a full `InferContext`
- Separation of concerns: the pre-pass produces a value, inference consumes it

**Cons:**
- Slightly more complex call site (`InferContext::new(registry)` instead of `InferContext::new()`)
- Pre-pass and inference must be coordinated by the caller (typechecker entry point)

## Decision

**Structure: Option A — `TypeDef` enum with typed lookup methods**  
**Location: Option B — Pre-built and injected**

The `TypeDef` enum approach over flat maps because every real caller (struct literal, field access, enum variant pattern) needs typed data, not just existence — so the indirection cost is zero in practice. The lookup methods encapsulate span-aware error production, keeping inference call sites clean. The v0.3 (generics) upgrade path is a single change inside `TypeRegistry` with no call-site impact.

The pre-built and injected approach over a field on `InferContext` because the registry is immutable after the pre-pass; embedding it in a mutably-borrowed struct misrepresents that invariant and creates borrow-checker friction if any lookup method ever returns a reference. Injection makes the construction sequence explicit and allows the registry to be unit-tested in isolation.

The two choices reflect a consistent design philosophy: the pre-pass produces fully-resolved, immutable state; `InferContext` consumes it and is responsible only for constraint accumulation and scope management.

## Consequences

- The typechecker entry point has a two-step construction sequence: `build_registry(&program)` → `InferContext::new(registry)`
- `InferContext::new` signature changes to accept a `TypeRegistry`
- The registry is read-only for the entire inference walk — no mutations after construction
- v0.3 (generics): extend `StructInfo`/`EnumInfo` field types from `InferType` to `TypeScheme`; `field_type` gains instantiation logic; call sites unchanged
- v0.3 (traits): add `impls` to `StructInfo`/`EnumInfo` or a parallel `ImplTable`; registry structure accommodates either without restructuring

## References

- Stage 4 (struct/enum registry for StructLiteral, FieldAccess, Match) — v0.1, now complete
- Mutable binding tracking — v0.1, now complete
- v0.3 — Generics: will extend field types to TypeScheme
- v0.3 — Traits: will add impl storage to registry
