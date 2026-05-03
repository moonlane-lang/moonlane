# Task 0002: Implement Type Inference Engine

**Status:** in-progress  
**Epic:** epic-001-typechecker  
**Component:** typechecker  
**Spec Link:** docs/01-SPEC/LANGUAGE-SPEC.md#32-type-inference  
**Blocked By:** 0001

## What

Implement the Hindley-Milner type inference engine in `src/typeinference/`.
Built across 8 incremental phases — each phase fully tested before the next begins.

Implementation notes and worked examples for each phase live in
`docs/03-COMPONENTS/typeinference/ROADMAP.md`.

## Phases

### Phase 1 — Type Variables ✓
`TypeVar(u32)` newtype and `TypeVarGenerator`. Foundation for all later phases.  
**Files:** `src/typeinference/mod.rs`  
**Tests:** `tests/typeinference_tests.rs` → `phase_1_type_variables`

### Phase 2 — Inference Types
`InferType` enum — types that may contain unresolved type variables, used during
inference before a final `Type` is known.

### Phase 3 — Unification
Core algorithm: given two `InferType`s, produce a `Substitution` that makes them
equal, or error. Includes occurs check to prevent infinite types.

### Phase 4 — Substitution
`Substitution` struct (`HashMap<TypeVar, InferType>`) with `bind`, `lookup`,
`apply` (recursive replacement), and `compose`.

### Phase 5 — Constraints
`Constraint { lhs, rhs, span }` and `solve_constraints(Vec<Constraint>)`.
Batch-solving produces better error messages than eager unification.

### Phase 6 — Type Schemes (let-polymorphism)
`TypeScheme { quantified_vars, ty }` with `generalize` (identify which vars to
quantify) and `instantiate` (fresh variables per use site).

### Phase 7 — Inference Context
`InferContext` — the state threaded through inference: `TypeVarGenerator`, mono
and poly environments, constraint accumulator, current substitution.

### Phase 8 — Integration with Typechecker
Wire `InferContext` into `typechecker::check()`. Walk the untyped AST, emit
constraints, solve, and produce a `TypedProgram`.

## Acceptance Criteria

### Phase 1 ✓
- [x] `TypeVar` struct implemented (newtype over `u32`)
- [x] `TypeVarGenerator::fresh()` produces sequential unique variables
- [x] `Display` formats as `?t0`, `?t1`, etc.
- [x] Ordering and hashing derived
- [x] All `phase_1_type_variables` tests pass

### Phase 2 — InferType
- [ ] `InferType` enum: `Concrete(Type)`, `Var(TypeVar)`, `Fun`, `Tuple`, `Array`, `Named`
- [ ] `Display` implemented
- [ ] Helper constructors: `int()`, `float()`, `bool()`, `str()`, `unit()`, `var(v)`
- [ ] `phase_2_infer_types` tests pass

### Phase 3 — Unification
- [ ] `unify(a: &InferType, b: &InferType) -> Result<Substitution, YolangError>`
- [ ] Concrete types must be identical to unify
- [ ] Variable binds to any type (occurs check first)
- [ ] Function, tuple, array, named types unify component-wise
- [ ] Occurs check prevents `?t0 = Array(?t0)`
- [ ] `phase_3_unification` tests pass

### Phase 4 — Substitution
- [ ] `Substitution::new()`, `bind(var, ty)`, `lookup(var)`
- [ ] `apply(ty) -> InferType` replaces all variables recursively
- [ ] `compose(other) -> Substitution`
- [ ] `phase_4_substitution` tests pass

### Phase 5 — Constraints
- [ ] `Constraint { lhs: InferType, rhs: InferType, span: Span }`
- [ ] `solve_constraints(constraints) -> Result<Substitution, YolangError>`
- [ ] Type errors include source location from `span`
- [ ] `phase_5_constraints` tests pass

### Phase 6 — Type Schemes
- [ ] `TypeScheme { quantified_vars: Vec<TypeVar>, ty: InferType }`
- [ ] `generalize(ty, free_env_vars) -> TypeScheme`
- [ ] `instantiate(gen) -> InferType` (fresh vars for each use)
- [ ] `Display` renders as `∀?t0. fun(?t0) -> ?t0`
- [ ] `phase_6_type_schemes` tests pass

### Phase 7 — Inference Context
- [ ] `InferContext` struct
- [ ] `fresh_var() -> InferType`
- [ ] `bind_mono(name, ty)` and `lookup(name) -> Option<InferType>`
- [ ] `bind_poly(name, scheme)` — auto-instantiates on lookup
- [ ] `add_constraint(lhs, rhs, span)`
- [ ] `solve() -> Result<Substitution, YolangError>`
- [ ] `phase_7_infer_context` tests pass

### Phase 8 — Integration
- [ ] `typechecker::check()` uses `InferContext` for a full inference pass
- [ ] Literals, variables, binary ops, function calls, let-bindings all infer correctly
- [ ] Type errors produce `YolangError::TypeError` with source span
- [ ] All `programs_tests` still pass
- [ ] No regressions in `typeinference_tests`

## Notes

- Run phase tests with: `cargo test --test typeinference_tests phase_N`
- Complete all tests for each phase before starting the next
- Phase 8 connects to task 0003 (type checker validation pass)
