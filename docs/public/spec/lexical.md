# Lexical Structure

## Comments

```moonlane
// Single-line comment

/* Multi-line
   comment */
```

Multi-line comments do not nest.

## Identifiers

Identifiers start with a letter (`a–z`, `A–Z`) or underscore, followed by any combination of letters, digits, or underscores.

```
identifier := [a-zA-Z_][a-zA-Z0-9_]*
```

By convention:
- Types, structs, enums, and aspects use `PascalCase`
- Variables, functions, and fields use `snake_case`

## Keywords

```
and       as        break     continue  else      enum      false
for       fun       if        impl      let       loop      match
mut       or        return    struct    aspect     true
use       where     while
```

## Literals

**Integers** — decimal, with optional `_` separators:
```moonlane
42
1_000_000
```

**Floats:**
```moonlane
3.14
2.0
```

Integer and float are distinct types and do not implicitly coerce.

**Strings** — double-quoted UTF-8:

| Sequence | Meaning         |
|----------|-----------------|
| `\n`     | Newline         |
| `\t`     | Tab             |
| `\\`     | Backslash       |
| `\"`     | Double quote    |
| `\r`     | Carriage return |

**Booleans:** `true`, `false`

**Absence literal:** `None`

## Operators

| Category        | Operators                                     |
|-----------------|-----------------------------------------------|
| Arithmetic      | `+`  `-`  `*`  `/`  `%`                       |
| Compound assign | `+=`  `-=`  `*=`  `/=`  `%=`                  |
| Comparison      | `==`  `!=`  `<`  `<=`  `>`  `>=`              |
| Logical         | `&&`  `\|\|`  `!`  (`and` / `or` as aliases)  |
| Assignment      | `=`                                           |
| Error prop      | `?`                                           |
| Type cast       | `as`                                          |
| Path            | `::`                                          |
| Range           | `..`  `..=`  (for use in `for-in` only)       |
