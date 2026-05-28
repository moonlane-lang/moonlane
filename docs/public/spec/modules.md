# Modules

## Files and Modules

Every `.mln` source file is a module. There is no `mod` declaration — the module graph is built entirely from `import` declarations.

The root file passed to the toolchain is the root module:

```bash
moonlane run src/main.mln
```

In that example, `root::` refers to `src/main.mln`.

## File-to-Module Mapping

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

`parser.mln` is the facade. Files in `parser/` form the namespace. There is no `name/mod.mln` convention.

## File Header Ordering

At file scope, `import` and `export` declarations must precede all other declarations:

```
(import | export)* declaration*
```

`import` and `export` are not valid inside blocks.

## Paths

Paths use `::` separators.

Path roots are:

| Root | Meaning |
|---|---|
| `root::` | The selected root module for the current program |
| `std::` | The bundled standard library root |
| `self::` | The current module |
| `super::` | The parent module; invalid from the root module |
| imported module handle | A module brought into scope by `import path::module;` |

Fully-qualified paths are valid anywhere a name is expected:

```moonlane
let token: root::parser::Token = root::parser::Token::new();
```

## Imports

`import` both loads the module file and brings names into the current scope:

```moonlane
import parser::{Ast, Token};       // loads parser.mln, brings Ast and Token into scope
import std::math;                  // loads std/math, brings math into scope as a module handle
import root::lexer::Token as Tok;  // absolute path with alias
import parser::*;                  // glob import — all public names from parser.mln
```

Import forms:

| Form | Effect |
|---|---|
| `import path::Name;` | imports `Name` |
| `import path::Name as Alias;` | imports `Name` under `Alias` |
| `import path::{A, B, C};` | imports multiple names from one path |
| `import path::{A as X, B};` | imports with per-item aliases |
| `import path::*;` | imports all public names from the module |
| `import path::module;` | imports `module` as a module handle; `module::item` is then valid |

## Re-exports

`export` re-exports names from submodules into the current module's public API:

```moonlane
// parser.mln — facade module for the parser namespace
export ast::Ast;
export lexer::{Token, Span};
export ast::ParseError as Error;
```

`export` and `import` share the same path and tree syntax. Re-exported names are indistinguishable from names defined directly in the re-exporting module.

`pub` and `export` serve different roles:

| Keyword | Purpose |
|---|---|
| `pub` | Marks a declaration in this file as externally accessible |
| `export path::Name;` | Re-exports a name from a submodule into this module's public API |

`export` declarations are processed after the module graph is fully loaded; they do not affect which files are loaded.

## Import Conflicts

Two explicit imports that bind the same local name in the same module are a compile-time error at the second import.

Glob imports use a softer rule:

- Local declarations beat glob imports.
- Explicit imports beat glob imports.
- Two glob imports may name the same item without an immediate error.
- A name from conflicting glob imports is an error only if it is referenced ambiguously.

## Visibility

Declarations are module-private by default. A declaration is accessible from outside its module only if it is annotated with `pub`.

```moonlane
pub struct Token { kind: TokenKind, span: Span }
struct InternalState { count: Int }

pub fun parse(tokens: Token[]) -> Ast { ... }
fun helper(token: Token) -> Bool { ... }
```

`pub` is valid on `struct`, `enum`, `fun`, `linear struct`, `linear enum`, `aspect`, and top-level `let`/`mut` bindings.

In v0.5.0, fields of a `pub struct` are public. Fields of a private struct are private because the struct itself is not externally nameable.

Within a module, all names defined in that module are accessible without qualification, including private names.

Modules do not have their own visibility annotation. Module-level access control is handled entirely by `pub` on individual items.

## Circular Imports

Circular imports are a compile error. The error message includes the full import chain.

## Module Graph Loading

The module graph is built from `import` declarations:

1. The root file is parsed.
2. All `import` declarations are collected; each is resolved to a file path via the `::` → `/` mapping.
3. Each referenced file is loaded recursively; cycles are detected and rejected.
4. Only files reachable via at least one `import` declaration are loaded.

`export` declarations do not affect which files are loaded.

## Single-File Compatibility

A `.mln` file with no `import` or `export` declarations is a complete program. Existing single-file programs remain valid without modification.
