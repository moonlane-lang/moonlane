---
id: rfc-0017
title: "Language Edition System"
date: '2026-05-22'
status: draft
---

## Summary

Introduce a per-module `edition` directive that pins a source file to a specific spec version. The interpreter and future compiler enforce the declared edition's syntax and semantics for that module independently of what edition the rest of the program targets. When a future version of the language introduces a breaking change, existing code that declares an older edition continues to work without modification.

---

## Motivation

The [versioning model](../versioning.md) explicitly permits breaking changes before v1.0. Without a mechanism for code to declare the edition it was written for, any breaking change requires every existing source file to be updated in lockstep. This is acceptable when all code lives in one project and the author can update everything, but it becomes untenable once:

- the module system (RFC-0009) lands and programs span multiple files or packages authored by different people,
- library code is distributed and cannot be updated by the consumer,
- the language introduces a change that is syntactically backward-compatible but semantically different (silent breakage is worse than a compile error).

An edition system decouples the evolution of the language from the update cycle of existing code. Each module declares the spec version it was written for; the toolchain applies exactly those rules when processing it.

---

## Proposal

### Directive syntax

An `edition` directive is the first non-comment statement in a source file:

```moonlane
edition "0.1";
```

The string literal is a spec version in `MAJOR.MINOR` form matching the version numbering in `versioning.md`. The directive is optional in v0.1 (to keep existing single-file programs valid) but required once a module system exists and inter-edition imports are possible.

`edition` is added to the reserved keyword list in the lexical spec.

### Omission behavior

If no `edition` directive appears, the toolchain defaults to `"0.1"` and emits a warning recommending an explicit declaration. This rule ensures existing v0.1 programs are valid without change, and that the absence of a directive is never silently ambiguous.

### What an edition governs

An edition pins the complete behaviour of a module as described by the corresponding spec version:

- **Syntax**: which constructs are legal to write.
- **Semantics**: what those constructs mean at runtime.
- **Type rules**: which programs are well-typed, including inference and checking behaviour.
- **Error codes**: the set of diagnostics and their meanings.

An edition does **not** affect the public interface a module exposes. Types and functions exported by a module are represented in a shared, edition-neutral form at module boundaries (see *Cross-module semantics* below).

### Cross-module semantics

When module A imports module B and A and B declare different editions, each module is processed under its own edition's rules. The interpreter/compiler is responsible for translating values at the boundary:

- Types exported from B are interpreted according to B's edition. A receives them as opaque types if they do not exist in A's edition, or as the edition-A equivalent if a mapping is defined.
- Calling a function exported by B from within A: argument types are checked against B's declared signature using B's type rules; return values enter A under A's type rules.
- A breaking change in edition X.Y is, by definition, a change that requires a new edition to take effect. Code that does not opt in to the new edition is never exposed to the change.

The specific translation rules for each edition boundary are defined in the changelog entry for the version that introduced the change.

### Supported edition window

- **Pre-v1.0**: all editions from v0.1 onward are supported. The toolchain may emit deprecation warnings for editions older than the previous minor version.
- **At v1.0 and beyond**: the minimum supported edition advances at most once per major version, with at least one full major version of deprecation notice.

The set of supported editions is documented alongside each interpreter and compiler release.

### Interaction with RFC-0009 (Module System)

The edition directive is a file-level attribute, not a module-level declaration. When RFC-0009 defines how files map to modules, the edition declared in a file governs exactly that file. If a module spans multiple files (if RFC-0009 permits that), each file declares its own edition independently; the module's public interface is still edition-neutral.

### Grammar change

Add the following production before any declaration:

```
file       = edition_decl? statement*
edition_decl = "edition" string_literal ";"
```

`edition` is added to the keyword table in `spec/lexical.md`.

---

## Alternatives Considered

**Project-level edition (single edition per build).** Simpler tooling: no per-file tracking, no cross-edition translation. The tradeoff is that it forces all files in a project to update together when the edition changes, which may be a problem when consuming third-party modules written for an older edition.

**Deprecation warnings only, no edition semantics.** Warn when old patterns are used but continue to compile them. This avoids the complexity of per-module semantics but may not be sufficient for genuine breaking changes — the moment a construct must mean something different in the new version, a warning alone does not preserve correctness.

**Automatic migration tool, no runtime edition semantics.** Provide a tool that rewrites old code to the new edition, à la `cargo fix`. This could be complementary to an edition system rather than a replacement — migration tools are useful but may not cover all cases (generated code, pinned dependencies, intentional use of old semantics).

---

## Open Questions

- **Edition validation**: should the toolchain reject an `edition` value it does not recognise (hard error), or fall back to the nearest known edition (soft degradation)?
- **Minimum edition for new features**: if a new builtin or type is added in v0.2, is it accessible from a module declaring `edition "0.1"`? Proposal: new additions are available in all editions unless the addition itself is a syntax change that conflicts with the old grammar.
- **Edition and the REPL**: does an interactive session have a default edition, or does it inherit from a project config?
- **Edition in the grammar file**: `src/grammar.pest` is shared across all editions today. Long-term, does the grammar need to be versioned too, or is it always the latest superset?

---

## Timing Recommendation

This RFC should be accepted and implemented before the first breaking change is introduced — at the latest, before v0.3. The directive syntax is a small parser change; the cross-module semantics are only exercised once RFC-0009 lands. A practical sequencing is:

1. Accept this RFC.
2. Add `edition` to the lexer and parser (trivial, no semantic effect yet).
3. When RFC-0009 ships, wire up per-module edition enforcement.
4. When the first breaking change is introduced, define the boundary translation rules for that change.

---

## References

- Language spec: `docs/public/spec.md`
- Versioning model: `docs/internal/versioning.md`
- RFC-0009 Module System: `docs/internal/rfcs/rfc-0009-module-system.md`
