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

Before `check_graph` runs, a dedicated normalization pass rewrites qualified path expressions to a new `Expr::ResolvedPath` AST node:

```
load_root → normalize → check_graph → evaluate_graph
```

`normalize(ModuleGraph) -> Result<ModuleGraph, MoonlaneError>` (in `src/path_normalizer.rs`) walks every `Expr::Path` node and rewrites it using the module's `ResolvedNames`.

```rust
Expr::ResolvedPath {
    resolved: String,       // bare name the typechecker uses for lookup
    original: Vec<String>,  // original segments, used in error messages
    span: Span,
}
```

| Expression | `resolved` | `original` |
|---|---|---|
| `parser::Token` | `"Token"` | `["parser", "Token"]` |
| `root::parser::Token` | `"Token"` | `["root", "parser", "Token"]` |
| `self::compute` | `"compute"` | `["self", "compute"]` |

Single-segment paths pass through as plain `Expr::Path`. Unresolvable qualified paths are a hard error before the typechecker runs.

The typechecker looks up `resolved` for name resolution and uses `original.join("::")` when constructing error messages. This is explicit in the type: ignoring `original` in an error site is a visible omission. It also survives inferred-type error messages, where there is no source span for the type itself — the `ResolvedPath` node carries the original form independently of span text.

### Type-checking loop

`check_graph` takes a `StdPrelude` parameter (#188) which seeds `GlobalExports` with `std::` and `core` schemes before the per-module loop begins. All other modules have been loaded by the file loader, which errors on any missing file (#186). Together these two invariants guarantee that every import in every `LoadedModule` has a corresponding `GlobalExports` entry by the time scope construction starts. A missing entry at that point is an internal error, not a user error.

Each module produces a `ModuleExports { scheme_env: SchemeEnv, type_env: HashMap<String, Type> }` bundle that is accumulated into `GlobalExports`. When typechecking module M, the inference context is pre-seeded with:
1. M's own declarations.
2. For each `import mod::name`, the corresponding entry from `GlobalExports[mod]`.
3. For `import mod::*`, all `pub` entries from `GlobalExports[mod]`.

Before inference runs, all `pub`-marked declarations in M are validated to have explicit type annotations (#187, error code `T0010`). This ensures exported schemes are fully concrete and consumable by downstream modules without cross-module type inference.

### Private-item error: `T0009`

Accessing a name that exists but is not `pub` in the source module produces error code `T0009`. The message names the item and the module it belongs to:

```
error[T0009]: `Token` is private in module `lexer`
```

This is distinct from `T0003` (undefined name) — the name is known; it is merely inaccessible.

## Migration Path

1. Implement `check_graph` (returns `TypedModuleGraph`) with `StdPrelude` parameter; define `TypedModule`/`TypedModuleGraph` types; add topological order `debug_assert!` to `load_root` (Issue #172, #188).
2. Implement `evaluate_graph` alongside the existing `evaluate` (Issue #183).
3. Make missing module files a hard load error; `std::` remains loader-transparent (Issue #186).
4. Wire `ResolvedNames` from the `ModuleGraph` into each module's inference scope (Issue #173).
5. Implement the path normalization pass `src/path_normalizer.rs` (Issue #185).
6. Enforce `pub_surface` in glob and named imports; introduce `T0009` (Issues #174, #176).
7. Require explicit type annotations on `pub` declarations; introduce `T0010` (Issue #187).
8. Add alias resolution (Issue #175).
9. Add conflict detection (Issue #177).
10. Add re-export propagation with visibility constraint — only `pub` names in source may be re-exported (Issue #178).
11. Migrate CLI binary to new pipeline (Issue #184).
12. Remove the flat-merge `load_program`, `check(Program)`, `evaluate(TypedProgram)`, and all ADR-0019/ADR-0020 fallback code (Issue #179).
13. Update spec and changelog; mark RFC-0030 incorporated (Issue #180).
14. Per-module runtime context in evaluator — deferred to v0.7.0 (Issue #189).

## Resolved Questions

1. **Output shape:** `check_graph` returns `TypedModuleGraph`. The evaluator is updated in the same sprint. The flat `TypedProgram` path is deleted when the migration is complete (Issue #179).

2. **Private-item error code:** New code `T0009` — "name is private in module X". Using `T0003` ("undefined name") would be misleading since the name is known to the typechecker.

3. **Qualified path expressions in code:** Handled by the path normalization pass (#185), not by the typechecker. Qualified `Expr::Path` nodes are replaced with `Expr::ResolvedPath { resolved, original }` — `resolved` is the bare name used for lookup, `original` is the full qualified form used in error messages. This is explicit in the AST type rather than relying on span text, which would be fragile for inferred-type error messages where no span exists for the type itself.

4. **Silent-skip for unresolvable imports:** Removed. The loader (#186) errors on missing files; `std::` modules are pre-loaded by the typechecker. There is no legitimate case where an import silently produces nothing — every import either resolves or is an error.

5. **Std pre-loading informality:** `check_graph` takes an explicit `StdPrelude` parameter (#188) with `StdPrelude::default()` and `StdPrelude::empty()` constructors. Tests that do not need std pass `StdPrelude::empty()` for isolation. The convention of "typechecker does this first" is replaced by a typed, required argument.

6. **Alias + normalizer interaction:** When `import mod::name as alias` is in scope, `mod::name` as an expression rewrites to `alias` — the local binding — not the bare `name`. Writing `mod::name` without an alias for the bare form is a normalizer error.

7. **Unannotated pub declarations:** `pub` declarations without explicit type annotations produce `T0010` before inference runs (#187). This enforces the no-cross-module-inference invariant at the point where it would otherwise silently produce incomplete exported schemes.

8. **Re-export of private names:** A `pub import` may only re-export a name that is `pub` in the source module. Attempting to re-export a private name is `T0009` (#178). This prevents visibility leaks through facade modules.

9. **Topological ordering implicit:** `ModuleGraph::modules` is documented as a topological ordering guarantee, and `load_root` adds a `debug_assert!` that validates it at construction time (#172). A violation surfaces immediately as an assertion failure during development rather than as a silent wrong-order typecheck.

10. **Evaluator flat runtime:** Acknowledged as a known deferral. `evaluate_graph` concatenates `TypedDecl` lists in v0.6.0. Per-module runtime context is tracked in #189 for v0.7.0.
