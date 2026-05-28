# Interpreter Architecture

> Rationale for the tree-walk approach: [decisions/adr-0004-interpreter-architecture.md](decisions/adr-0004-interpreter-architecture.md)

## Pipeline

```
.mln root source file
       │
       ▼
  ┌───────────────┐
  │ Module Loader │  selected root file → module graph; invokes parser per file
  └───────────────┘
       │  ast::Program
       ▼
  ┌──────────────┐
  │ Type Checker │  untyped AST → typed AST  (errors reported here)
  └──────────────┘
       │  typed_ast::TypedProgram
       ▼
  ┌─────────────┐
  │  Evaluator  │  typed AST → program output  (tree-walking)
  └─────────────┘
```

Each stage is a separate Rust module. No stage is skipped.

---

## Crate Structure

```
tree-walk-interpreter/
├── Cargo.toml
└── src/
    ├── main.rs            — CLI entry point: selects a root .mln file, runs the pipeline
    ├── grammar.pest       — pest PEG grammar for the language
    ├── module_loader.rs   — loads the selected root file and its transitive import graph
    ├── name_resolver.rs   — resolves import scopes, visibility, and re-exports per module
    ├── parser/            — drives pest, builds untyped AST from CST
    ├── ast/               — untyped AST node definitions
    ├── types/             — concrete type representation (Type enum)
    ├── typeinference/     — HM inference engine: type vars, unification, constraints, schemes
    ├── typechecker/       — two-pass type checker; produces typed AST
    │   ├── mod.rs         — check() entry point, SchemeEnv alias, FunGeneralization
    │   ├── registry.rs    — build_registry, register_builtins, concrete env builders
    │   ├── inference.rs   — Pass 1: all infer_* functions
    │   ├── construction.rs— Pass 2: ConstructCtx, construct_* functions, exhaustiveness
    │   └── conversions.rs — type_expr_to_infer, infer_type_to_type, type_to_infer
    ├── typed_ast/         — typed AST node definitions
    ├── evaluator/         — tree-walking evaluator, environment, runtime values
    │   ├── mod.rs         — core: Value, Signal, Environment, evaluate(), eval_block/stmt/expr
    │   ├── builtins.rs    — register_builtins: all built-in function bindings
    │   ├── call.rs        — call_function, call_function_mut_self
    │   ├── display.rs     — format_float, value_to_display_string, format_value
    │   ├── lvalue.rs      — eval_binop, apply_assign_op, lvalue path helpers
    │   └── pattern.rs     — match_pattern
    └── error/             — unified error type covering all pipeline stages
```

---

## Component Boundaries

| Data | Type | Produced by | Consumed by |
|------|------|-------------|-------------|
| Module graph | `module_loader::ModuleGraph` | module loader | CLI / tests |
| Untyped program | `ast::Program` | parser / module loader | typechecker |
| Typed program | `typed_ast::TypedProgram` | typechecker | evaluator |
| Errors | `MoonlaneError` | any stage | caller / CLI |

---

## Error Design

All errors use a unified `MoonlaneError` type:

```rust
enum MoonlaneError {
    ParseError   { code: ErrorCode, message: String, start: usize, end: usize, filename: String },
    TypeError    { code: ErrorCode, message: String, start: usize, end: usize, filename: String },
    RuntimePanic { message: String, start: usize, end: usize, filename: String },
    Internal     { message: String },
}
```

Type error codes: E0001–E0008. Runtime panics (`.yolo()` on `nope`, out-of-bounds, division by zero) terminate with a non-zero exit code.

---

## Component Notes

| Component | Notes |
|-----------|-------|
| Module Loader | `src/module_loader.rs` — `load_root` builds the `ModuleGraph`; `load_program` flattens it into a merged `ast::Program` |
| Name Resolver | `src/name_resolver.rs` — `resolve` produces per-module `ModuleScope` and a `pub_surface` map; not yet wired into the `check` pipeline (v0.5.0) |
| Parser | `src/parser/`, `src/grammar.pest` |
| Type Checker | [typechecker.md](typechecker.md) |
| Evaluator | [evaluator.md](evaluator.md) |
| Design decisions | [decisions/](decisions/) |
