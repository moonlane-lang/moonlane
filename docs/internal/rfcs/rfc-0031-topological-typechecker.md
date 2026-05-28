---
number: 0031
title: Topological Per-Module Typechecking
status: draft
created: 2026-05-28
milestone: v0.6.0
tracking_issue: "#172"
---

# RFC-0031: Topological Per-Module Typechecking

## Motivation

The v0.5.0 module loader builds a `ModuleGraph` in topological order (dependencies before dependents), but the typechecker ignores this structure. It receives a flat `Program` that concatenates every module's declarations and type-checks them all in a single pass (ADR-0019). This flat merge:

- Prevents visibility enforcement — every declaration is globally visible regardless of `pub`
- Prevents import-scoped resolution — `import mod::name` has no effect on what names are in scope
- Prevents conflict detection — two modules exporting the same name silently collide
- Requires the last-segment fallback hack (ADR-0020) to resolve qualified paths

The v0.6.0 module-semantic sprint must replace the flat merge with a topological multi-pass typechecker that processes each module against its own declared scope.

## Goals

1. The typechecker receives a `ModuleGraph` (already in topological order) instead of a flat `Program`.
2. Each module is typechecked in isolation: only names in scope (from its imports and its own declarations) are visible.
3. A module's `pub` declarations become available to downstream modules after it is checked.
4. The flat merge (`module_loader::load_program`) and last-segment fallback (`ADR-0020`) are removed.
5. Qualified path expressions in code (`root::mod::Name`, `self::name`) are resolved to bare local bindings before typechecking, in a dedicated normalization pass.

## Non-Goals

- Incremental or parallel typechecking (future work).
- Cross-module type inference (type variables do not flow across module boundaries in v0.6.0; all public APIs must be fully annotated).
- The standard library (`std::`) — deferred to a later sprint.

## Design Options

Three approaches are viable. Each is described below with its trade-offs.

---

### Option A — Multi-pass with a shared scheme registry

**Structure:**

```
for module in graph.modules (topological order):
    scope = build_scope(module, already_checked_exports)
    (typed_module, exports) = typecheck_module(module, scope)
    already_checked_exports.insert(module.path, exports)
```

Each `typecheck_module` call runs the existing HM inference engine but against a `TypeEnv` seeded only with the module's own declarations plus the names it explicitly imports.

A `SchemeRegistry` accumulates every module's exported `SchemeEnv` entry as it is checked, so later modules can consume them.

**Trade-offs:**

- **+** Minimal change to the inference engine. The existing `InferContext` / `ConstructCtx` APIs are reused.
- **+** Clear ownership: each module produces a self-contained export bundle.
- **−** Requires threading `ModuleGraph` all the way into `typechecker::check`, which currently takes a `Program`. The public API must change.
- **−** The `Program` type (which carries flat `decls`) is no longer the canonical input; a transitional shim is needed until all callers are updated.

**Recommended for:** straightforward correctness, incremental migration possible.

---

### Option B — Pre-phase name resolution wired into the inference context

**Structure:**

Run `name_resolver::resolve()` for every module in the graph before the inference pass begins. Feed the resulting `ResolvedNames` (already computed by the loader) into each module's `InferContext` as its initial environment.

```
resolved_map: HashMap<ModulePath, ResolvedNames> = ...  // already in ModuleGraph
for module in graph.modules:
    env = ResolvedNames_to_TypeEnv(&resolved_map[&module.path])
    typed = infer_with_env(module.decls, env)
```

**Trade-offs:**

- **+** Reuses `name_resolver.rs` which is already fully implemented and unit-tested.
- **+** `ResolvedNames` already carries `pub_surface`, `imports`, `aliases` — all the data needed.
- **−** `ResolvedNames` is currently a flat name → source-module map, not a type-scheme map. It would need to grow type information, creating a coupling between name resolution and type inference.
- **−** Two-phase resolution (names first, types second) can diverge if a name resolves to a declaration whose type is only known after inference.

**Recommended for:** if name_resolver.rs is the preferred single source of truth for all scope questions.

---

### Option C — Incremental module contexts (lazy export propagation)

**Structure:**

Rather than a strict topological sweep, build a `ModuleContext` per module lazily: when module A needs to typecheck a reference to `b::Foo`, it requests B's export map, which triggers B's typechecking if not yet done.

```
fn get_export(ctx: &mut GlobalCtx, path: &[String], name: &str) -> Option<Scheme> {
    if !ctx.checked.contains_key(path) {
        ctx.typecheck_module(path);  // recursive
    }
    ctx.exports[path].get(name)
}
```

**Trade-offs:**

- **+** Natural demand-driven evaluation — only what is reachable is checked.
- **+** Cycle detection falls out naturally (a module requesting its own export during checking is a cycle).
- **−** Requires mutable global context passed by reference into recursive calls — awkward in Rust without RefCell/Mutex.
- **−** Order of side-effects is harder to reason about; error messages may arrive in non-deterministic order.
- **−** Complexity cost is high relative to the benefit for the current codebase size.

**Not recommended** for v0.6.0; revisit if the graph grows large enough that eager topological order becomes a bottleneck.

---

## Recommended Approach

**Option A** is the recommended approach for v0.6.0.

### Output type: `TypedModuleGraph`

`check_graph` returns a `TypedModuleGraph` — a per-module typed AST — rather than a merged `TypedProgram`. The evaluator is updated in the same sprint to accept `TypedModuleGraph` as its entry point.

```rust
pub struct TypedModule {
    pub module_path: Vec<String>,
    pub decls: Vec<TypedDecl>,
}

pub struct TypedModuleGraph {
    pub root: Vec<String>,
    pub modules: Vec<TypedModule>,  // topological order
}

pub fn check_graph(graph: ModuleGraph) -> Result<TypedModuleGraph, MoonlaneError>
pub fn evaluate_graph(graph: TypedModuleGraph) -> Result<(), MoonlaneError>
```

The old `check(Program)` and `evaluate(TypedProgram)` are kept as compatibility wrappers (single-module synthetic graph) until all callers are migrated, then deleted alongside the flat-merge hack.

### Path normalization pass

Before `check_graph` runs, a dedicated normalization pass rewrites qualified path expressions in the AST to bare local bindings:

```
load_root → normalize → check_graph → evaluate_graph
```

`normalize(ModuleGraph) -> Result<ModuleGraph, MoonlaneError>` (in `src/path_normalizer.rs`) walks every `Expr::Path` node and rewrites it using the module's `ResolvedNames`:

| Expression | Rewrite |
|---|---|
| `parser::Token` | `Token` |
| `root::parser::Token` | `Token` |
| `self::compute` | `compute` |
| `super::util::helper` | `helper` (if imported) |

If a qualified path cannot be resolved to any in-scope binding, the normalizer errors immediately with a clear message. Single-segment paths pass through unchanged.

This keeps path-resolution concerns out of the type inference engine entirely. The typechecker only ever sees bare names. Qualified path syntax in error messages is preserved via the original span.

### Type-checking loop

Each module produces a `ModuleExports { scheme_env: SchemeEnv, type_env: HashMap<String, Type> }` bundle that is accumulated into a `GlobalExports` registry. When typechecking module M, the inference context is pre-seeded with:
1. M's own declarations.
2. For each `import mod::name`, the corresponding entry from `GlobalExports[mod]`.
3. For `import mod::*`, all `pub` entries from `GlobalExports[mod]`.

Imports whose source module is not in `GlobalExports` (unresolvable `std::`, `root::`, or `super::` imports) are silently skipped during scope construction. Usage of the unresolved name fails at inference time with T0003.

### Private-item error: `T0009`

Accessing a name that exists but is not `pub` in the source module produces error code `T0009`. The message names the item and the module it belongs to:

```
error[T0009]: `Token` is private in module `lexer`
```

This is distinct from `T0003` (undefined name) — the name is known; it is merely inaccessible.

## Migration Path

1. Implement `check_graph` (returns `TypedModuleGraph`) alongside the existing `check` (Issue #172).
2. Implement `evaluate_graph` alongside the existing `evaluate` (Issue #183).
3. Wire `ResolvedNames` from the `ModuleGraph` into each module's inference scope (Issue #173).
4. Implement the path normalization pass `src/path_normalizer.rs` (Issue #185).
5. Enforce `pub_surface` in glob and named imports; introduce `T0009` (Issues #174, #176).
6. Add alias resolution (Issue #175).
7. Add conflict detection (Issue #177).
8. Add re-export propagation (Issue #178).
9. Migrate CLI binary to new pipeline (Issue #184).
10. Remove the flat-merge `load_program`, `check(Program)`, `evaluate(TypedProgram)`, and all ADR-0019/ADR-0020 fallback code (Issue #179).
11. Update spec and changelog; mark RFC-0030 incorporated (Issue #180).

## Resolved Questions

1. **Output shape:** `check_graph` returns `TypedModuleGraph`. The evaluator is updated in the same sprint. The flat `TypedProgram` path is deleted when the migration is complete (Issue #179).

2. **Private-item error code:** New code `T0009` — "name is private in module X". Using `T0003` ("undefined name") would be misleading since the name is known to the typechecker.

3. **Qualified path expressions in code:** Handled by the path normalization pass (#185), not by the typechecker. The typechecker receives only bare names after normalization. This keeps path-resolution logic out of the inference engine and out of the `TypeEnv` (no qualified aliases needed). The restructuring of the typechecker into an explicit multi-stage pipeline is deferred; the normalization pass is a standalone module that does not require internal typechecker changes.
