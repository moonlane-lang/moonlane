# Modules

## Files and Module Declarations

A source file is a module. Modules are declared explicitly with `mod` declarations at the top of a file:

```moonlane
mod parser;
pub mod lexer;
```

`mod name;` resolves to either `name.mln` or `name/mod.mln` alongside the declaring file. If both files exist, the program is rejected as ambiguous.

A bare `mod name;` declares a private submodule. `pub mod name;` declares a public submodule whose path is reachable from outside the declaring module.

The selected root file is the root module. In script mode, the selected root file is the file passed directly to the toolchain:

```bash
moonlane run src/main.mln
```

In that example, `root::` refers to `src/main.mln`.

## File Header Ordering

At file scope, all module declarations must come before all imports, and all imports must come before other declarations:

```text
mod* use* declaration*
```

`mod` or `use` declarations are not valid inside blocks.

## Paths

Paths use `::` separators.

Path roots are:

| Root | Meaning |
|---|---|
| `root::` | The selected root module for the current program or package |
| `std::` | The bundled standard library root |
| `self::` | The current module |
| `super::` | The parent module; invalid from the root module |
| imported module name | A module handle introduced by `use path::module;` |
| declared child module name | A child module declared by `mod name;` in the current module |

Fully-qualified paths are valid anywhere a name is expected:

```moonlane
let token: root::parser::Token = root::parser::Token::new();
```

## Imports

Imports bring public names into the current module:

```moonlane
use std::math;
use std::collections::{Map, Set};
use root::parser::Ast;
use root::v1::Parser as ParserV1;
use root::prelude::*;
```

Import forms:

- `use path::to::Name;` imports `Name`.
- `use path::to::{A, B, C};` imports multiple names from one path.
- `use path::to::Name as Alias;` imports `Name` under `Alias`.
- `use path::*;` imports all public names from the module.
- `use path::module;` imports `module` as a module handle, allowing `module::item`.

`pub use` re-exports an imported name from the current module's public API:

```moonlane
mod ast;
mod lexer;

pub use ast::Ast;
pub use lexer::Token;
```

Re-exported names are indistinguishable from names defined directly in the re-exporting module.

## Import Conflicts

Two explicit imports that bind the same local name in the same module are a compile-time error.

Glob imports use a softer ambiguity rule:

- Local declarations beat glob imports.
- Explicit imports beat glob imports.
- Two glob imports may export the same name without an immediate error.
- A name from conflicting glob imports is an error only if it is referenced ambiguously.

## Visibility

Declarations are module-private by default. A declaration is accessible from outside its module only if it is annotated with `pub`.

```moonlane
pub struct Token { kind: TokenKind, span: Span }
struct InternalState { count: Int }

pub fun parse(tokens: Token[]) -> Ast { ... }
fun helper(token: Token) -> Bool { ... }
```

`pub` is valid on `mod`, `struct`, `enum`, `fun`, `linear struct`, `linear enum`, `aspect`, and top-level `let`/`mut` bindings.

In v0.5.0, fields of a `pub struct` are public. Fields of a private struct are private because the struct itself is not externally nameable.

Within a module, all names defined in that module are accessible without qualification, including private names.

## Single-File Compatibility

A `.mln` file with no `mod` or `use` declarations is a complete program. Existing single-file programs remain valid without modification.
