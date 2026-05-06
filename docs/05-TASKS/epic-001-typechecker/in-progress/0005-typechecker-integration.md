# Task 0005: Integrate Type Inference into Typechecker

**Status:** in-progress  
**Epic:** epic-001-typechecker  
**Component:** typechecker  
**Spec Link:** docs/01-SPEC/LANGUAGE-SPEC.md#32-type-inference  
**Blocked By:** 0002

## What

Wire `InferContext` (from task 0002) into `typechecker::check()` to produce a
real `TypedProgram` from an untyped `Program`, and connect that output to the
evaluator so the full pipeline runs end-to-end.

This covers both *inference* (determining types) and *validation* (rejecting
ill-typed programs). In a constraint-based system these are not separate passes —
constraints both determine and validate simultaneously. A separate validation pass
is therefore not needed.

This is a multi-stage implementation: not all AST node types will be handled
immediately. Each stage adds coverage for more nodes and is tested with `.yolo`
programs in `tests/test_programs/`.

## Architecture

### Two-Pass Design

**Pass 1 — Inference**: Walk the AST with `InferContext`, emitting constraints
and returning `InferType`s. Solve all constraints at the end with `ctx.solve()`.

**Pass 2 — Construction**: Walk the AST again with the final `Substitution`,
converting `InferType → Type` and building `TypedExpr` / `TypedDecl` nodes.

> **Optimization note**: The two-pass approach visits the AST twice. A future
> optimization is to carry a "pending typed node" alongside the `InferType`
> during pass 1 (substituting the final type in-place after solving), avoiding
> the second traversal. Not worth the complexity now.

### Pre-Pass — Function Registration

Before inference begins, scan all top-level `FunDecl`s and register their names
with fresh type variables in the `InferContext`. This allows forward references
and mutual recursion. Concrete types are unified during the inference pass when
the bodies are walked.

### Type Conversion Rules

**`TypeExpr → InferType`** (for explicit annotations):
- `Named("Int", [])` → `InferType::int()`
- `Named("Float", [])` → `InferType::float()`
- `Named("Bool", [])` → `InferType::bool()`
- `Named("String", [])` → `InferType::str()`
- `Unit` → `InferType::unit()`
- `Tuple(ts)` → `InferType::Tuple(...)`
- `Array(t)` → `InferType::Array(...)`
- `Fun(params, ret)` → `InferType::Fun(...)`
- Other `Named` → `InferType::Named(name, args)` (user-defined types)

**`InferType → Type`** (after solving, for TypedAST construction):
- `Concrete(t)` → `t`
- `Var(v)` → **error**: "cannot infer type for `?tN`"
- Structural variants → recurse

### BinOp Constraint Rules

Operators are checked strictly via constraints now. In the future, arithmetic
operators will be dispatched through traits (e.g. `Add`, `Sub`) — at that point
this logic should be replaced by trait constraint generation.

> **Future note**: When traits are implemented, `a + b` should generate a
> `T: Add<Output=R>` trait constraint rather than a direct numeric type check.
> The current rules are a deliberate simplification.

Current rules:
- `+`, `-`, `*`, `/`, `%` — both operands must be numeric (`Int` or `Float`),
  result has the same type as the operands
- `==`, `!=`, `<`, `<=`, `>`, `>=` — both operands must be the same type,
  result is `Bool`
- `&&`, `||` — both operands must be `Bool`, result is `Bool`
- `..`, `..=` — both operands must be `Int`, result is a range (use `Named("Range", [Int])`)

### Unsupported Node Handling

AST nodes not yet implemented return `YolangError::Internal` with a clear
"not yet supported" message, so unimplemented features fail loudly rather than
silently producing wrong types.

## Implementation Stages

### Stage 1 — Core expressions and let-bindings
- Literals (`Int`, `Float`, `Bool`, `Str`, `Unit`, `Nope`)
- Identifiers (lookup in `InferContext`)
- `BinOp` with constraint rules above
- `UnaryOp` (`-` on numeric, `!` on bool)
- `Decl::Let` and `Decl::Mut` with optional type annotation
- `Decl::Fun` with typed parameters and return type
- **Tests:** `tests/test_programs/phase8_stage1_*.yolo`

### Stage 2 — Control flow statements
- `Stmt::If` (condition must be `Bool`)
- `Stmt::While` (condition must be `Bool`)
- `Stmt::Return`
- `Stmt::Expr`
- `Expr::If` (both branches must unify)
- **Tests:** `tests/test_programs/phase8_stage2_*.yolo`

### Stage 3 — Composite expressions
- `Expr::Tuple`
- `Expr::Array`
- `Expr::Call`
- `Expr::Index`
- **Tests:** `tests/test_programs/phase8_stage3_*.yolo`

### Stage 4 — Advanced expressions
- `Expr::Closure`
- `Expr::Match`
- `Expr::MethodCall`
- `Expr::FieldAccess`
- `Expr::StructLiteral`
- `Stmt::For`, `Stmt::ForIn`, `Stmt::Loop`
- **Tests:** `tests/test_programs/phase8_stage4_*.yolo`

## Acceptance Criteria

### Stage 1
- [ ] `TypeExpr → InferType` conversion covers all annotation forms
- [ ] `InferType → Type` conversion errors on unresolved variables
- [ ] Pre-pass registers all top-level function names
- [ ] Literals infer to their concrete types
- [ ] Identifiers resolve via `InferContext::lookup`
- [ ] `BinOp` emits correct constraints and produces correct result type
- [ ] `UnaryOp` emits correct constraints
- [ ] `Decl::Let` / `Decl::Mut`: annotation (if present) is unified with inferred value type
- [ ] `Decl::Fun`: parameters and return type unified, body inferred, scheme generalized
- [ ] Stage 1 `.yolo` test programs pass through `check()` without error
- [ ] Type mismatches produce `YolangError::TypeError` with source span

### Stage 2–4
- [ ] (to be filled in as stages begin)

### Final — Evaluator integration
- [ ] `typechecker::check()` output passes into `evaluator::evaluate()` without error
- [ ] Full pipeline `parse() → check() → evaluate()` works on a non-trivial program
- [ ] All previous tests still pass

## Testing

Tests are `.yolo` source files in `tests/test_programs/` run through the full
`parse() → check()` pipeline. Each file is named after its stage and what it
tests. Negative tests (expected type errors) use a `// ERROR` comment convention
to be defined when the test harness is set up.

## Open Questions

### Mutual recursion in nested scopes
The pre-pass registers top-level `FunDecl`s before inference begins, covering
most cases. But functions declared inside blocks (e.g. inside another function's
body) are not visible to the pre-pass. Two options: (a) do a recursive pre-pass
at every block entry, or (b) restrict mutual recursion to top-level only for now.
Decision needed before Stage 2.

### Generic function parameters
`FunDecl` carries `generics: Vec<GenericParam>`. During inference, do generic
params become fresh type variables (treated like local unknowns), or do we
refuse to infer generic functions until a separate generics phase? If we treat
them as fresh vars, generalization via `TypeScheme` may produce incorrect schemes
when bounds are involved.

### `Nope` literal type
`Literal::Nope` is Yolang's null/None equivalent. Its type should be
`Perhaps(?t0)` for a fresh `?t0` — making it polymorphic. But this means a bare
`let x = Nope` leaves `?t0` unresolved, which under the current rules would be
an error. Do we special-case it, require an annotation, or introduce a default?

### `loop` expression type
A `loop { break 42; }` has type `Int`. A loop that never breaks has type
"never" (`!`). We don't currently have a `Type::Never` variant. Do we add one,
or treat non-breaking loops as `Unit` for now?

### Block return type vs `return` statement
A block's type is its tail expression, or `Unit` if none. But a block can also
exit early via `return`. The inferred type of the block tail and the type of
`return` values must both unify with the function's declared return type. How do
we thread the expected return type through the inference context? Options: store
it on `InferContext`, or pass it as an explicit parameter to block/statement
inference.

### Struct and enum type registry
`Decl::Struct` and `Decl::Enum` define new named types. When we later handle
`FieldAccess` and `StructLiteral`, we need to resolve field types. Where does the
struct/enum registry live — on `InferContext`, as a separate `TypeEnv`, or in a
dedicated pre-pass? This affects the architecture of Stage 4.

### Negative test convention
The task says `.yolo` files with expected errors use a `// ERROR` comment, but
the exact convention is undefined. Options: (a) a comment on the line that errors
(`// ERROR: cannot unify`), (b) a file-level annotation (`// EXPECT_ERROR`), or
(c) a separate `.error` sidecar file. Needs a decision before negative tests are
written.

### `Path` expression resolution
`Expr::Path(["Foo", "Bar"])` could be an enum variant (`Foo::Bar`), a module
path, or a static method. Type inference for paths requires knowing which it is.
Do we defer path resolution to a separate name-resolution pass before inference,
or handle it inside the typechecker?

### Cast expression validity
`expr as TargetType` — do we validate that the cast is legal (e.g. `Int as Float`
is fine, `Bool as Int` may not be), or do we trust the programmer and just assign
the target type unconditionally for now?

### Assign statement type checking
`x = expr` requires that `expr`'s type unifies with `x`'s current type. For
compound assignment (`x += 1`), we also need the BinOp rules to apply. Does
assignment generate a constraint against `x`'s existing `InferType` in the
environment, or does it require a separate mutable binding tracker?

### Multiple error reporting
`solve_constraints` currently stops at the first unification failure. A better
user experience would collect all errors and report them together. This requires
a different solving strategy (continue past errors, collect them, return
`Vec<YolangError>` instead of `Result`). Decide when to tackle this — it is a
cross-cutting change that affects the error type and all call sites.

## Notes

- Run stage 1 tests: `cargo test --test programs_tests stage1`
- Unsupported nodes intentionally error — this is tracked, not hidden
- Inference and validation are unified in the constraint-based approach; no separate validation pass is needed (absorbed from task 0003)
