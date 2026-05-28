---
id: rfc-0030
title: "Module System Redesign"
date: '2026-05-28'
status: accepted
supersedes: [rfc-0009, rfc-0029]
---

## Summary

Replaces RFC-0009 and RFC-0029 with a revised module system that addresses the ergonomic shortcomings of the Rust-inspired design. The core problems with the previous design were: the required two-step `mod` + `use` pattern for every imported module, the `name/mod.mln` directory module convention, and `pub use` as the re-export mechanism. This RFC resolves all three with minimal added complexity.

**Supersedes:** RFC-0009 (Module System), RFC-0029 (Module System — Gaps and Clarifications)  
**Target:** v0.5.0

---

## Motivation

The RFC-0009 design required two separate declarations to use a module:

```moonlane
mod parser;               // declares the module exists (loads the file)
use parser::{Ast, Token}; // brings names into scope
```

Both steps are mandatory. Skipping `mod` means the file is never loaded. Skipping `use` means you can only access names via fully-qualified paths. This double-declaration pattern was the primary ergonomic complaint.

Additionally, `pub use` reads as a mechanism (`pub` + `use`) rather than an intent (`export`), and `name/mod.mln` as the directory module entry point directly imports a Rust convention that has no other motivation.

---

## Design

### `import` replaces both `mod` and `use`

A single `import` declaration both loads the module file and brings names into the current scope:

```moonlane
import parser::{Ast, Token};       // loads parser.mln, brings Ast and Token into scope
import std::math;                  // loads std/math, brings math into scope as a module handle
import root::lexer::Token as Tok;  // absolute path with alias
import parser::*;                  // glob import — all public names from parser.mln
```

There is no `mod` keyword and no `use` keyword for module imports. `import` is the only form.

Import forms:

| Form | Effect |
|---|---|
| `import path::Name;` | imports `Name` |
| `import path::Name as Alias;` | imports `Name` under `Alias` |
| `import path::{A, B, C};` | imports multiple names from one path |
| `import path::{A as X, B};` | imports with per-item aliases |
| `import path::*;` | imports all public names from the module |
| `import path::module;` | imports `module` as a module handle; `module::item` is then valid |

### `export` replaces `pub use`

Re-exporting names from submodules uses an explicit `export` declaration:

```moonlane
// parser.mln — facade module for the parser namespace
export ast::Ast;
export lexer::{Token, Span};
export ast::ParseError as Error;
```

`export` and `import` share the same path and tree syntax. `export` re-exports into the current module's public API; the exported names are then accessible as if defined directly in the re-exporting module.

`pub` on declarations continues to mark individual items as externally accessible. `pub` and `export` serve different roles:

| Keyword | Purpose |
|---|---|
| `pub` | Marks a declaration in this file as externally accessible |
| `export path::Name;` | Re-exports a name from a submodule into this module's public API |

### File-to-module mapping

`::` maps directly to `/` in the filesystem. There is no special directory module file.

| Import | File resolved |
|---|---|
| `import parser::Ast;` | `parser.mln` |
| `import parser::ast::Ast;` | `parser/ast.mln` |
| `import root::a::b::c::T;` | `a/b/c.mln` relative to the root file |

A directory module with a public facade is expressed by placing `name.mln` alongside the `name/` directory. The two coexist without ambiguity — they are different paths:

```
src/
  main.mln            ← import parser::Ast; import parser::lexer::Token;
  parser.mln          ← export ast::Ast; export lexer::Token;
  parser/
    ast.mln           ← pub struct Ast { ... }
    lexer.mln         ← pub struct Token { ... }
```

`parser.mln` is the facade. Files in `parser/` form the namespace. There is no `parser/mod.mln` convention.

### Module visibility

Modules do not have their own visibility annotation. Module-level access control is handled entirely by `pub` on individual items. If an item is `pub`, its full path is accessible to any importer. If it is private, it is not.

To hide the internal file structure from importers, a parent module uses `export` to expose only the names it chooses:

```moonlane
// parser.mln
export ast::Ast;          // Ast is accessible as root::parser::Ast
export lexer::Token;      // Token is accessible as root::parser::Token
                          // parser/ast.mln and parser/lexer.mln paths remain accessible
                          // but callers are expected to use the facade
```

There is no equivalent of `pub mod` / private mod from RFC-0009. This simplification is intentional for v0.5.0. Path-level module privacy is deferred.

### Paths

Path roots are unchanged from RFC-0029:

| Root | Meaning |
|---|---|
| `root::` | The selected root module for the current program |
| `std::` | The bundled standard library root |
| `self::` | The current module |
| `super::` | The parent module; invalid from the root module |
| imported module handle | A module brought into scope by `import path::module;` |

Fully-qualified paths are valid anywhere a name is expected without a preceding `import`:

```moonlane
let p: root::parser::Ast = root::parser::Ast::new();
```

`import` is a local binding convenience, not the only access mechanism.

### File header ordering

```
(import | export)* declaration*
```

`import` and `export` declarations may appear in any order relative to each other, but all must precede any other declarations. `import` and `export` are not valid inside blocks.

### Import conflicts

Explicit import conflicts (two `import` statements binding the same local name) are a compile error at the second import.

Glob imports use a softer rule:
- Local declarations beat glob imports.
- Explicit imports beat glob imports.
- Two glob imports may name the same item without an immediate error; using that name is an error only if the reference is ambiguous.

### Circular imports

Circular imports are a compile error. The error message includes the full import chain.

### Module graph loading

The module graph is built from `import` declarations. The loader:

1. Parses the root file.
2. Collects all `import` declarations; resolves each to a file path via `::` → `/` mapping.
3. Recursively loads each referenced file, detecting cycles.
4. Only files reachable via at least one `import` declaration are loaded.

`export` declarations are processed after the graph is fully loaded. They do not affect which files are loaded.

### std::core auto-import

Unchanged from RFC-0029: `std::core` is auto-imported into every file as if `import std::core::*;` appeared implicitly. The auto-import is lowest priority; any explicit `import` beats it. A local declaration shadows the auto-import in its declaring module only.

### Single-file compatibility

A `.mln` file with no `import` or `export` declarations is a complete program. Fully preserved.

---

## Grammar changes

```
file         ::= header-decl* declaration*
header-decl  ::= import-decl | export-decl
import-decl  ::= 'import' import-path ';'
export-decl  ::= 'export' import-path ';'
import-path  ::= path-root '::' import-tree
               | path-root
path-root    ::= 'root' | 'std' | 'self' | 'super' | identifier
import-tree  ::= import-item
               | '{' import-item (',' import-item)* '}'
               | '*'
               | identifier '::' import-tree
import-item  ::= identifier ('as' identifier)?
pub-ann      ::= 'pub'   -- unchanged; valid on struct, enum, fun, let, mut, linear struct, linear enum, aspect
```

`mod`, `use`, and `pub use` are removed from the grammar.

---

## Changes from RFC-0009 / RFC-0029

| RFC-0009/0029 | RFC-0030 |
|---|---|
| `mod name;` declares a submodule | removed — `import` builds the module graph |
| `pub mod name;` makes a submodule public | removed — no module-level visibility annotation |
| `use path::Name;` brings a name into scope | `import path::Name;` |
| `pub use path::Name;` re-exports a name | `export path::Name;` |
| `name/mod.mln` as directory module entry point | removed — `name.mln` alongside `name/` directory |
| File header: `mod* use* declaration*` | `(import\|export)* declaration*` |
| Glob import `use path::*;` | `import path::*;` — same conflict rules |
| `use path::Name as Alias;` | `import path::Name as Alias;` |
| `super::`, `self::` path roots | unchanged |
| `root::` path root | unchanged |
| Circular import is a compile error | unchanged |
| `pub` on declarations | unchanged |
| `std::core` auto-import | unchanged |
| Single-file compatibility | unchanged |

---

## Open Questions

None — all questions from RFC-0009 and RFC-0029 are either resolved by this RFC or unchanged.

---

## Decision

**Outcome:** Accepted  
**Target:** v0.5.0

The Rust-inspired `mod` + `use` two-step was the primary ergonomic shortcoming of RFC-0009. Collapsing both into `import` eliminates the pattern without adding complexity elsewhere. `export` as an explicit re-export keyword is cleaner than `pub use`. Dropping `name/mod.mln` removes a Rust convention with no independent motivation.

The removal of module-level visibility (`pub mod`) is the most significant simplification. Item-level `pub` is sufficient for v0.5.0; path-level module privacy can be added later if the need arises in practice.
