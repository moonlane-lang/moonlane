# Typechecker Implementation Notes

> Status: v0.1 complete.  
> Extension points for v0.3 (generics and aspects) are called out inline.

---

## Pipeline Position

```
untyped AST  ──►  check()  ──►  TypedProgram
                    │
                    ├─ Pre-pass: register builtins, enums, hoist names
                    ├─ Pass 1:   infer — emit constraints, solve
                    └─ Pass 2:   construct — re-derive concrete types, build TypedAST
```

Entry point: `typechecker::check(program: Program) -> Result<TypedProgram, MoonlaneError>`

---

## Module Structure

| File | Responsibility |
|---|---|
| `mod.rs` | `check()` entry point, `SchemeEnv` alias, `FunGeneralization` struct |
| `registry.rs` | `build_registry`, `register_builtins`, `build_concrete_*_env` |
| `inference.rs` | Pass 1 — all `infer_*` functions |
| `construction.rs` | Pass 2 — `ConstructCtx`, all `construct_*` functions, exhaustiveness checking |
| `conversions.rs` | `type_expr_to_infer`, `infer_type_to_type`, `resolved_to_type`, `type_to_infer` |

The inference engine lives in `src/typeinference/` (type vars, unification, substitution, constraints, schemes). The typechecker modules in `src/typechecker/` walk the AST and drive that engine.

---

## Theory Background

### Type Variables and InferType

During inference, types may be partially unknown. **Concrete types** (`Type` enum) are fully resolved: `Int`, `String`, `fun(Int) -> String`. **Inference types** (`InferType` enum) may contain type variables — placeholders that get unified with concrete types as more information becomes available:

```
?t0, ?t1, ?t2   — type variables (generated fresh for each unknown)
fun(?t0) -> ?t1 — an InferType containing type variables
```

Type variables satisfy an **occurs check**: `?t0` cannot be unified with `Array(?t0)`, which would create an infinite type.

### Unification

Unification makes two types equal by binding type variables:

```
unify(Int, Int)                        → ok, already equal
unify(?t0, Int)                        → ok, bind ?t0 = Int
unify(?t0, ?t1)                        → ok, bind ?t0 = ?t1
unify(Int, String)                     → error, incompatible
unify(fun(?t0) -> ?t0, fun(Int) -> Int) → ok, bind ?t0 = Int
unify(?t0, Array(?t0))                 → error, occurs check
```

### Substitution and Constraints

A **substitution** is a map from type variables to types (`?t0 → Int`, `?t1 → String`). Applying a substitution replaces all variables in a type with their bindings.

Rather than unifying types immediately as the AST is walked, the inference system collects **constraints** (equality relations between `InferType`s, each tagged with a source span) and solves them in batch. Batch solving handles complex interdependencies and produces better error messages.

### Let-Polymorphism and Type Schemes

A **type scheme** is a type with universally quantified variables: `∀α. α → α` (the identity function — works with any type). In code:

```rust
pub struct TypeScheme {
    pub quantified_vars: Vec<TypeVar>,
    pub ty: InferType,
}
```

When `let id = fun(x) { x }` is inferred:
1. Infer the body — parameter `x` gets fresh variable `?t0`, function type is `fun(?t0) -> ?t0`
2. **Generalize**: identify free type variables not shared with the outer environment; `?t0` is free → scheme `∀?t0. fun(?t0) -> ?t0`
3. Bind `id` to this scheme in `poly_env`

When `id` is **used**, the scheme is **instantiated** with fresh type variables — each call site gets an independent copy:

```
id(42)      → instantiate to fun(?t1) -> ?t1, unify ?t1 = Int  → id(42) : Int
id("hello") → instantiate to fun(?t2) -> ?t2, unify ?t2 = String → id("hello") : String
```

### The Generalization Boundary: Why `env_fvs` Matters

Generalization must only quantify type variables that are *truly local* to the function. If a variable is shared with the outer scope, quantifying it is unsound.

```moonlane
fun f(x) {
    let g = fun(y) { x };   // g's type: fun(?t1) -> ?t0 where ?t0 is x's type
}
```

`?t1` is local to `g` — safe to quantify. `?t0` is shared with `f`'s scope — quantifying it would let different calls to `g` return different types, but `x` has one concrete type per call to `f`. The typechecker snapshots the environment's free variables (`env_fvs`) before entering the function body and only quantifies variables absent from that set:

```
fun_ty   = fun(?t1) -> ?t0
env_fvs  = {?t0}                          ← x's type is already in scope
scheme   = ∀?t1. fun(?t1) -> ?t0          ← ?t0 left free, not quantified
```

The snapshot is taken before pushing the function's parameter scope — the right moment, capturing what the surrounding context has already committed.

### Never Type

`InferType::Never` (the bottom type `!`) unifies with any type. Diverging expressions — `return`, `break`, `continue`, and infinite `loop` with no reachable `break` — produce `Never`. This lets the constraint solver treat dead branches as compatible with any expected type.

### Rank-1 Limitation

The HM algorithm infers types at rank 1: `∀` only at the outermost level. Higher-rank polymorphism (e.g. a function that accepts a polymorphic function as an argument) requires decidability-breaking extensions and is not supported. The practical consequence: function arguments are unified as monotypes; passing a polymorphic function as an argument works only if the call site knows the concrete instantiation.

---

## Pre-Pass

Three hoisting steps run before Pass 1:

1. `register_builtins` — binds built-in function names (`print`, `array_push`, etc.) as `TypeScheme` entries in `ctx.poly_env`, and registers `String.len` in `ctx.method_env`.
2. `build_registry` (via `TypeRegistry`) — registers `Perhaps<T>` and `Result<T,E>` with their type params as fresh type variables, user-defined enum variants, struct field types, and method signatures.
3. `hoist_fun_decls` — walks top-level `FunDecl`s and pre-registers each with a fresh type variable in `ctx.mono_env`. Enables forward references and mutual recursion.

`hoist_fun_decls` is also called at block entry in `infer_block`, so nested functions support forward references within their block.

Struct and enum declarations are registered globally by `build_registry`, regardless of where they appear in the source. `build_registry` recursively walks all function bodies (and nested blocks — `While`, `For`, `ForIn`) in addition to the top-level declaration list, so a `struct` declared inside a function body is registered in the same global `TypeRegistry` as a top-level `struct`. This means locally-declared structs are **visible across the entire compilation unit**, not just the enclosing function. There is currently no scope concept in the registry.

---

## Pass 1 — Type Inference

**Modules:** `typeinference/mod.rs` (engine) + `typechecker/inference.rs` (AST walkers)

### Environment Structure

```
InferContext {
    mono_env: Vec<HashMap<String, (InferType, bool)>>  // scope stack, innermost last
    poly_env: HashMap<String, TypeScheme>               // flat, top-level polymorphic bindings
    constraints: Vec<Constraint>                        // accumulated equality constraints
    var_gen: TypeVarGenerator                           // globally unique TypeVar allocator
    registry: TypeRegistry                              // pre-built struct/enum/method registries
    current_return_type / current_break_type            // context for return/break inference
}
```

`poly_env` takes precedence over `mono_env` in `ctx.lookup()`. Poly entries are automatically instantiated with fresh type variables on each lookup (let-polymorphism).

### Constraint Emission

Each `infer_expr` call returns an `InferType` and may push zero or more `Constraint`s into `ctx.constraints`. Constraints are not solved inline — they accumulate and are solved in batch.

### Inline Solve-and-Generalize (Functions)

`infer_fun_decl` solves accumulated constraints immediately after inferring the function body, generalizes the function type, and re-binds it as a `TypeScheme` in `poly_env`. This is essential for:
- Let-polymorphism: the function's type scheme can be instantiated fresh at each call site
- Mutual recursion: the pre-hoisted mono binding is unified with the inferred type before generalization

The same constraints remain in `ctx.constraints` after the inline solve; the final `ctx.solve()` at the top level re-solves the same list (idempotent).

### Eager Partial Solves

A few inference cases call `ctx.solve()` eagerly to determine structural type information before emitting further constraints:

- `Expr::ForIn`: resolves the iterable type to decide Array vs Range
- `Expr::FieldAccess`, `Expr::MethodCall`, `Expr::TupleAccess`: resolves the receiver type to look up fields/methods

These partial solves are read-only (they produce a `Substitution` value but don't modify `ctx.constraints`). They are a pragmatic workaround for the fact that field/method lookup requires knowing the concrete type name — a fundamental limitation of constraint-only inference.

### Type Ascription (`:` Operator)

`e : T` is a pure inference hint. Inference:
1. Infers the inner expression type `inner_ty`.
2. Converts the annotation `T` to an `InferType` via `type_expr_to_infer`.
3. Adds a constraint `inner_ty ~ ascribed_ty`.
4. Returns `inner_ty` (not the annotated type directly).

The constraint propagates the annotation into the solver without changing control flow. In Pass 2, the ascription node is **erased**: `construct_expr` resolves the annotation to a concrete `Type` and constructs the inner expression with that type as the expected-type hint. No `TypedExpr::Ascribe` variant exists — ascription has zero runtime cost.

---

## Pass 2 — Construction

**Module:** `typechecker/construction.rs`

Pass 2 re-walks the untyped AST with:
- `subst: &Substitution` — the final solved substitution from Pass 1
- `scheme_env: &SchemeEnv` — generalized type schemes for user-defined functions
- `ConstructCtx` — a stripped-down context with concrete `Type` values (no inference)

Each `construct_expr` call re-derives the node's concrete type by applying `subst` to the inferred type and converting via `infer_type_to_type`. No constraints are emitted; no unification is performed.

### Polymorphic Call Sites

When a call site resolves to a polymorphic callee (present in `scheme_env` but not in `ConstructCtx.env`), `construct_call` calls `instantiate_scheme_for_call`, which:
1. Instantiates the scheme with fresh type variables
2. Unifies the instantiated param types against the concrete argument types
3. Returns the concrete `Fun` type for the specific call

### Polymorphic Function Bodies

Functions with quantified type variables in their scheme are stored as `FunBody::Generic(untyped_block)` rather than `FunBody::Typed(typed_block)`. This is a placeholder for v0.3 monomorphization — not a working generic dispatch mechanism.

### Exhaustive Match Checking

`check_match_exhaustiveness` runs at the end of `construct_match` once the scrutinee type is known concretely.

- An unguarded `_`, bare binding pattern, or irrefutable tuple `(a, b, ...)` is a catch-all — exhaustive.
- **Guarded arms do not count**: a guard may fail at runtime.
- `Bool`: must cover `true` and `false` (both unguarded).
- `Perhaps(_)`: must cover `Perhaps::Some` and `Perhaps::Nope`.
- `Result(_, _)`: must cover `Result::Ok` and `Result::Err`.
- Named enum: must cover every variant.
- `Never`: vacuously exhaustive.
- All other types (Int, Float, Str, …): value-infinite; only a catch-all satisfies exhaustiveness.

Error: `E0008 Non-exhaustive match`.

---

## Type Registries

Three registries live inside `TypeRegistry` (owned by `InferContext`):

| Field | Type | Content |
|---|---|---|
| `struct_env` | `HashMap<String, Vec<(String, InferType)>>` | struct name → field list |
| `method_env` | `HashMap<String, HashMap<String, InferType>>` | type name → method name → fun type |
| `enum_env` | `HashMap<String, EnumInfo>` | enum name → variant list + type params |

`TypeRegistry` is constructed in one pre-pass and injected into `InferContext::new`, consistent with [ADR-0001](decisions/adr-0001-typeregistry-structure-and-location.md).

---

## Known Limitations

### `as` Cast — Widening Only (Provisional)

`Int as Float` (widening) and identity casts are supported. Narrowing (`Float as Int`) and cross-type casts are rejected. v0.3 (#12) replaces the fixed-case check with a `From<S>` aspect lookup.

---

## Extension Points

### v0.3 — Generics

1. Change `struct_env` to carry type params (`StructInfo { type_params: Vec<TypeVar>, fields: … }`). Field lookup must instantiate type params with fresh vars (same pattern already used for `EnumInfo`).
2. Remove the `!fun.generics.is_empty()` error guard in `infer_fun_decl` and `infer_impl_method`. Implement proper generic function inference.
3. Replace `FunBody::Generic(untyped_block)` with monomorphization.
4. `let_polymorphism` (#10) is partially in place via `generalize/instantiate` — main work is generic structs and the monomorphization engine (#9).

### v0.3 — Aspects

1. Add `impl_env: HashMap<(String, String), Vec<MethodInfo>>` (type × aspect → methods) or extend `TypeRegistry` with aspect-impl storage.
2. Replace the provisional `as` cast with a `From<S>` aspect check.
3. Replace the provisional `?` error type match with a `From<E>` coercion lookup.
4. Upgrade `for-in` from Array/Range only to an `Iterable<T>` aspect check (#11).
