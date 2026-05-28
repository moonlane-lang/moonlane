# ADR-0024: T0009 Visibility Enforcement Lives in the Typechecker, Not the Name Resolver

**Status:** Accepted  
**Date:** 2026-05-28  
**Tracking issues:** #174, #191

---

## Context

When a module imports a name from another module, the interpreter must reject private-item access (T0009) and distinguish it from importing a name that does not exist at all (T0003).

Two plausible placements were considered:

1. **Name resolver**: check visibility when recording import bindings. This requires `pub_surface` to list every declared name (not just public ones) so the resolver can tell T0009 from T0003.
2. **Typechecker** (`build_import_schemes`): record all import bindings unconditionally in the name resolver; check visibility in the typechecker, which has access to the full `NormalizedModuleGraph` and can scan the source module's `program.decls` directly.

---

## Decision

Visibility enforcement is done in the typechecker's `build_import_schemes`, not in the name resolver.

The name resolver records every import binding regardless of the source item's visibility:

```rust
// Record the binding regardless of visibility.
// Visibility (T0009) and existence (T0003) are checked by the typechecker
// in build_import_schemes, which has access to the full graph and GlobalExports.
(base.to_vec(), BindingKind::Item)
```

`build_import_schemes` then scans the source module's `program.decls` to distinguish:
- Name exists but is private → T0009
- Name does not exist → T0003

---

## Why Not the Name Resolver

The name resolver processes imports using only `pub_surface` (the set of declared public item names per module). It cannot distinguish "name exists but is private" from "name does not exist" without also having access to all declared names, which would require duplicating the full declaration list in `ResolvedNames`.

An earlier implementation attempt added `all_decl_names: HashMap<ModulePath, HashSet<String>>` to `ResolvedNames` for this purpose. This was rejected as an unnecessary coupling — the typechecker already iterates `graph.modules` and has access to all `program.decls` without needing a pre-built index.

---

## Invariant

The name resolver must **not** gate bindings on visibility. Any filtering it applies will prevent the typechecker from generating the correct error code. The name resolver's contract is: record where names come from, not whether they are accessible.
