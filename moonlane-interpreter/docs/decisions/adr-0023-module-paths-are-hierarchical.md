# ADR-0023: Module Paths Are Hierarchical (Absolute from Root)

**Status:** Accepted  
**Date:** 2026-05-28  
**Updated:** 2026-05-28 (v0.6.0 — extended to cover all PathRoot variants)

---

## Context

The module loader assigns a `module_path: Vec<String>` to each loaded module. This path is used as the key in `GlobalExports`, `ResolvedNames.scopes`, `ResolvedNames.pub_surface`, and in scope binding lookups.

---

## Decision

Module paths are **absolute from the project root**, constructed by concatenating the parent module's path with the new module's name segments:

```
root.mln          →  module_path = []
root imports parser → module_path = ["parser"]
parser imports lexer → module_path = ["parser", "lexer"]
```

This is enforced in `module_loader.rs` by `child_module_path`, which mirrors `name_resolver::absolute_base` for every `PathRoot` variant:

| PathRoot | `absolute_base` result | `child_module_path` result |
|---|---|---|
| `Name(n)` | `current + [n]` | `parent + mod_segs` (where `mod_segs = [n] + rest`) |
| `Root` | `[]` | `mod_segs` only (no parent prefix) |
| `Self_` | `current` | `parent + mod_segs` |
| `Super` | `current[..-1]` | `parent[..-1] + mod_segs` |

This ensures `import root::helper::*` from `parser.mln` (path `["parser"]`) resolves to module path `["helper"]`, matching helper's actual `module_path` — **not** `["parser", "helper"]`.

The original implementation only handled `PathRoot::Name` correctly (`parent + mod_segs`). The `root::`, `self::`, and `super::` cases were incorrect (also prepended parent path). The bug was discovered when the `root_qualified_path_in_non_root_module` integration test failed because `global_exports` had `["helper"]` but the loader registered it as `["parser", "helper"]`.

---

## Invariant

The `GlobalExports` key for a module **must equal** that module's `module_path` from the loader. Any code that computes a module identifier for lookup in `GlobalExports` or `ResolvedNames` must use the full hierarchical path, not just the last segment.

---

## Why Not Flat Paths

An earlier implementation used `PathRoot::Name(n) => vec![n.clone()]` (flat, single-segment). This worked for depth-1 modules (direct imports from root) but broke for deeper transitive imports: `parser/lexer` would get `module_path = ["parser", "lexer"]` but `absolute_base` would produce `["lexer"]`, causing `GlobalExports` and scope lookups to miss the module entirely.

The fix was discovered when the `transitive_dependency_via_graph_pipeline` integration test failed. The flat-path behavior was retained in unit tests that used manually-constructed graphs with non-hierarchical paths — those tests were updated to use hierarchical paths to match the loader's actual behavior.
