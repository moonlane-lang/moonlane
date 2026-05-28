---
id: rfc-0009
title: "Module System"
date: '2026-05-21'
status: superseded
superseded-by: rfc-0030
---

## Summary

Design the module system: how source files map to modules, how names are imported and exported, the `use` keyword semantics, visibility (`pub`), and re-exports (`pub use`). This is the largest deferred feature — it blocks the standard library, multi-file programs, and all visibility control.

---

## Motivation

All v0.1 programs are single-file. Adding a module system unlocks:

- Multi-file programs and code organisation
- A standard library (math, string, io, collections)
- Visibility control — `pub` to export, private by default
- Re-exports for public API shaping

The `use` keyword is already a reserved word in the grammar.

---

## Design

### File-to-module mapping

Modules are declared explicitly with `mod`. A `mod name;` statement in a file means the compiler looks for `name.mln` or `name/mod.mln` alongside the declaring file. If both candidate files exist, the program is rejected as ambiguous. The module tree is therefore explicit — not inferred from the filesystem — and the root is the selected root file.

```
src/
  main.mln       -- mod parser; mod evaluator;
  parser.mln     -- or: parser/mod.mln
  evaluator/
    mod.mln      -- mod expr; mod stmt;
    expr.mln
    stmt.mln
```

The `mod` declaration must appear at the top level of the declaring file. The declared module's contents are in the resolved file; they are not inlined into the declaring file.

A bare `mod name;` declares a private submodule. `pub mod name;` declares a public submodule whose path is reachable from outside the declaring module.

### `use` syntax

Imports use `::` path separators. The root of a path is one of:

- `root` — the selected root file for the current program or package
- `std` — the standard library
- `self` — the current module
- `super` — the parent module; invalid from the root module
- an imported module handle
- a bare child module name declared via `mod` in the current file

```moonlane
use std::math;
use std::collections::{Map, Set};
use root::parser::Ast;
use root::parser::{Ast, Token, ParseError};
use root::v1::Parser as ParserV1;
```

- `use path::to::Name` — imports `Name` into the current scope
- `use path::to::{A, B, C}` — imports multiple names from the same path
- `use path::to::Name as Alias` — imports `Name` under `Alias`
- `use path::to::*` — glob import (imports all `pub` names from the module)
- `use path::module` — imports `module` as a module handle, so `module::item` is valid
- All `use` statements must appear at the top level of a file, after `mod` declarations and before any other declarations

Fully-qualified paths such as `root::parser::Ast` are valid anywhere a name is expected; `use` is a local binding convenience, not the only way to access a public item.

Two explicit imports that bind the same local name in the same module are a compile error. Explicit imports and local declarations beat glob imports. Two glob imports may export the same name without an immediate error, but using that name is an ambiguity error unless an explicit import or local declaration resolves it.

### Visibility

All declarations are **module-private by default**. A declaration is accessible from outside its module only if annotated with `pub`.

```moonlane
pub struct Token { kind: TokenKind, span: Span }   // exported
struct InternalState { ... }                        // module-private

pub fun parse(tokens: Token[]) -> Ast { ... }      // exported
fun helper(t: Token) -> Bool { ... }               // module-private
```

`pub` is valid on: `mod`, `struct`, `enum`, `fun`, `linear struct`, `linear enum`, `aspect`, and top-level `let`/`mut` bindings.

In v0.5.0, fields of a `pub struct` are public. Fields of a private struct are private because the struct itself is not externally nameable. Field-level visibility is deferred to a follow-up design.

Within a module, all names (including private ones) are accessible without qualification. From outside the module, only `pub` names are accessible via their import path.

### `pub use` re-exports

A `pub use` statement re-exports a name from the current module's public API, regardless of where it was defined. This allows a module to shape its public interface independently of its internal file structure.

```moonlane
// parser/mod.mln
mod ast;
mod lexer;

pub use ast::Ast;          // Ast is now accessible as root::parser::Ast
pub use lexer::Token;      // Token re-exported from lexer submodule
                           // lexer itself remains private — not pub mod
```

Re-exported names are indistinguishable from names defined in the re-exporting module from the caller's perspective. This is the mechanism for facade modules and clean public API surfaces.

### Circular imports

Circular imports are a **compile error**. If module A imports from module B and module B imports from module A (directly or transitively), the compiler rejects the program with a clear cycle-detection error listing the import chain.

This enforces a directed acyclic dependency graph and keeps the module resolution algorithm simple.

### Standard library path

The standard library is accessible via the reserved `std` root. It is not a user-defined module and does not appear in `mod` declarations. The compiler resolves `std::*` paths to the bundled standard library regardless of the project structure.

```moonlane
use std::math;
use std::string;
use std::io;
use std::collections::Map;
```

User modules may not declare a top-level module named `std`.

### Single-file compatibility

Existing single-file programs are **fully valid** without modification. A `.mln` file with no `mod` or `use` declarations is a complete, self-contained program. The module system is purely additive — it is only activated when `mod` or `use` appears.

In a single-file program, the implicit module is the file itself. All top-level names are in scope without import.

Core types and aspects live in `std::core`, which is auto-imported into every file as if `use std::core::*;` appeared implicitly. This includes compiler-special core names such as `Perhaps`, `Result`, `Bool`, `Int`, `Float`, `String`, range types, and core aspects such as `Display`, `Iterable`, and `From`. User declarations may shadow auto-imported `std::core` names locally. Explicit imports also beat the auto-import.

---

## Grammar additions

```
file      ::= mod-decl* use-decl* declaration*
mod-decl  ::= 'mod' identifier ';'
            | 'pub' 'mod' identifier ';'
use-decl  ::= 'use' use-path ';'
            | 'pub' 'use' use-path ';'
use-path  ::= path-root '::' use-tree
path-root ::= 'root' | 'std' | 'self' | 'super' | identifier
use-tree  ::= identifier
            | identifier 'as' identifier
            | '{' use-tree (',' use-tree)* '}'
            | '*'
            | identifier '::' use-tree
pub-ann   ::= 'pub'   -- prefix on struct, enum, fun, let, mut declarations
```

---

## Open Questions

*(All resolved — see Decision section below.)*

---

## Decision

> **Superseded by RFC-0030** (2026-05-28). The `mod` + `use` two-step pattern and `name/mod.mln` directory module convention were replaced by a unified `import`/`export` design. See RFC-0030 for the accepted design.

**Outcome:** Accepted — v0.5.0 *(superseded before implementation)*

| Question | Decision |
|---|---|
| File-to-module mapping | Explicit `mod` declarations; `name.mln` or `name/mod.mln`; both existing is an ambiguity error |
| Module visibility | `mod` is private; `pub mod` makes the submodule path public |
| Root path | `root::` names the selected root file; script mode selects the file passed to the toolchain |
| `use` syntax | `use path::to::Name` with `::` separators; `use ... as` aliases; `use path::module` imports a module handle |
| Path roots | `root`, `std`, `self`, `super`, imported module handles, and declared child modules |
| Import conflicts | Explicit import conflicts are immediate errors; glob conflicts are errors only on ambiguous use |
| Visibility default | Private by default; `pub` to export |
| Struct fields | Fields of a `pub struct` are public in v0.5.0 |
| `pub use` re-exports | Included in v0.5.0 |
| Circular imports | Compile error |
| Standard library path | Reserved `std` root |
| Core prelude | `std::core` is auto-imported into every file |
| Single-file compatibility | Fully preserved — module system is additive |

**Target:** v0.5.0
