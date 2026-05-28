# Interpreter Architecture

> Rationale for the tree-walk approach: [decisions/adr-0004-interpreter-architecture.md](decisions/adr-0004-interpreter-architecture.md)

## Pipeline

```
.mln root source file
       │
       ▼
  ┌───────────────┐
  │ Module Loader │  root file → ModuleGraph (topological order); invokes parser per file
  └───────────────┘
       │  module_loader::ModuleGraph
       ▼
  ┌───────────────┐
  │ Name Resolver │  per-module import scopes, pub_surface, re-exports
  └───────────────┘
       │  name_resolver::ResolvedNames
       ▼
  ┌─────────────────┐
  │ Path Normalizer │  rewrites qualified Expr::Path nodes to Expr::ResolvedPath
  └─────────────────┘
       │  path_normalizer::NormalizedModuleGraph
       ▼
  ┌──────────────┐
  │ Type Checker │  per-module HM inference + construction (errors reported here)
  └──────────────┘
       │  typed_ast::TypedModuleGraph
       ▼
  ┌─────────────┐
  │  Evaluator  │  flattens TypedModuleGraph → program output  (tree-walking)
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
    ├── path_normalizer.rs — rewrites qualified Expr::Path nodes to Expr::ResolvedPath
    ├── parser/            — drives pest, builds untyped AST from CST
    ├── ast/               — untyped AST node definitions
    ├── types/             — concrete type representation (Type enum)
    ├── typeinference/     — HM inference engine: type vars, unification, constraints, schemes
    ├── typechecker/       — two-pass type checker; produces typed AST
    │   ├── mod.rs         — check() / check_graph() entry points, StdPrelude, GlobalExports
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
| Module graph | `module_loader::ModuleGraph` | module loader | name resolver / path normalizer |
| Resolved names | `name_resolver::ResolvedNames` | name resolver | path normalizer / typechecker |
| Normalized graph | `path_normalizer::NormalizedModuleGraph` | path normalizer | typechecker |
| Typed module graph | `typed_ast::TypedModuleGraph` | typechecker (`check_graph`) | evaluator (`evaluate_graph`) |
| Untyped program (single-file) | `ast::Program` | `load_program` (single-file shim) | typechecker (`check`) |
| Typed program (single-file) | `typed_ast::TypedProgram` | typechecker (`check`) | evaluator (`evaluate`) |
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
| Module Loader | `src/module_loader.rs` — `load_root` builds the topological `ModuleGraph`; `load_program` parses a single file (shim for single-file test harnesses) |
| Name Resolver | `src/name_resolver.rs` — `resolve` produces per-module `ModuleScope`, `pub_surface`, and re-exports; wired into `check_graph` |
| Path Normalizer | `src/path_normalizer.rs` — `normalize` rewrites qualified `Expr::Path` nodes to `Expr::ResolvedPath`; produces `NormalizedModuleGraph` |
| Parser | `src/parser/`, `src/grammar.pest` |
| Type Checker | [typechecker.md](typechecker.md) |
| Evaluator | [evaluator.md](evaluator.md) |
| Design decisions | [decisions/](decisions/) |
