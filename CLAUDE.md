# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

Moonlane is a statically typed, expression-oriented language with a Rust-inspired syntax. It runs in two first-class execution modes: a production-quality interpreter and a native compiler. Both are permanent, supported targets — the interpreter is not a stepping stone to be discarded when the compiler exists.

**This dual-mode commitment is the project's core identity and competitive position.** Design decisions must be consistent with it. See `docs/internal/vision.md` for the full rationale.

Key implications for agents working in this repo:
- Do not treat the tree-walk interpreter as throwaway scaffolding. It is a product.
- Do not design features that only make sense for a compiler unless explicitly designated compiler-only.
- Every language feature must answer: *what does this give the programmer in interpreter mode, and in compiler mode?*
- The spec is the contract both backends must satisfy. Ambiguity in the spec is a spec bug.

## Common Development Commands

### Building and Running
```bash
# Build the interpreter
cd tree-walk-interpreter
cargo build --release

# Run a Moonlane program
cargo run -- path/to/program.mln

# Run in debug mode with output
cargo run -- --debug path/to/program.mln
```

### Testing
```bash
# Run all tests
cargo test

# Run type inference tests
cargo test --test typeinference_tests

# Run typechecking tests
cargo test --test typechecking_tests

# Run specific test phase with output
cargo test --test typeinference_tests phase_2 -- --nocapture

# Run specific test by name
cargo test test_name -- --nocapture
```

### Development Workflow
```bash
# Lint and format (if available)
cargo clippy
cargo fmt

# Build and test together
cargo build && cargo test
```

## Project Architecture

The interpreter follows a multi-stage pipeline:

```
.mln source → Parser (pest) → AST Builder → Type Checker → Evaluator
```

### Key Components

- **Parser** (`src/parser/`): Uses pest PEG grammar (`src/grammar.pest`) to generate CST, then builds untyped AST
- **AST** (`src/ast/`): Untyped abstract syntax tree definitions
- **Type System** (`src/types/`, `src/typeinference/`, `src/typechecker/`): Type representation, inference engine, and validation
- **Typed AST** (`src/typed_ast/`): AST nodes that carry type information
- **Evaluator** (`src/evaluator/`): Tree-walking interpreter for executing typed programs
- **Error Handling** (`src/error/`): Comprehensive error types with source location tracking

### Working Directory

All Rust development happens in the `tree-walk-interpreter/` subdirectory. Always `cd` there first:

```bash
cd tree-walk-interpreter
# Then run cargo commands
```

## Agent Guide

See **[AGENTS.md](./AGENTS.md)** for the full agent workflow: task lifecycle, spec discipline, when to stop and ask, and decision record conventions. AGENTS.md is the authoritative guide for how to work in this repo.

## Documentation and Task Management

### Task Management — GitHub Projects v2

All tasks are tracked in **GitHub Projects v2** at https://github.com/users/Vladastos/projects/1. The project board is the canonical view for task status and planning.

Each issue has a **Status** field in the project:
- **Todo** — not yet started (`status:backlog`)
- **In Progress** — actively being worked (`status:in-progress`)
- **Done** — closed

The `status:*` labels mirror the project Status field and are kept in sync for CLI visibility. When an issue is closed, the `status:in-progress` label is removed automatically by a GitHub Actions workflow.

Use the `gh` CLI to manage issues:

```bash
gh issue list                                          # list open tasks
gh issue list --state closed                           # list done tasks
gh issue list --milestone "v0.5.0"                     # filter by milestone
gh issue view <number>                                 # read a task
gh issue create --title "..." --label "..." --milestone "..."  # create a task
gh issue close <number>                                # mark done
gh issue comment <number> --body "..."                 # add a note
gh issue edit <number> --add-label "status:in-progress"        # mark in-progress
```

**Labels:** `evaluator`, `generics`, `aspects`, `integration`, `tooling`, `dx`, `migration`, `docs`, `typechecker`, `type-inference`, `architecture`, `priority:low/medium/high`, `status:backlog`, `status:in-progress`, `archived`

**Milestones:** Version milestones (`v0.4.0`, `v0.5.0`, …). See [`docs/internal/versioning.md`](docs/internal/versioning.md) for the full model.

### Docs and Decisions

Spec documents, decision records, and RFCs live in `docs/`. Read them directly — no MCP tooling needed.

| Path | Purpose |
|---|---|
| `docs/public/spec.md` | **Language Specification** — entry point; links to all spec sections |
| `docs/public/spec/` | **Spec sections** — lexical, types, declarations, functions, expressions, runtime, grammar |
| `docs/public/changelog.md` | **Changelog** — per-version feature list |
| `docs/internal/rfcs/` | **RFCs** — language change proposals |
| `docs/internal/versioning.md` | **Versioning model** — version numbering, RFC lifecycle, doc conventions |
| `tree-walk-interpreter/docs/architecture.md` | **Architecture Overview** — pipeline diagram, component boundaries |
| `tree-walk-interpreter/docs/typechecker.md` | **Typechecker** — HM theory background + implementation notes |
| `tree-walk-interpreter/docs/evaluator.md` | **Evaluator** — runtime values, signals, environment, known limitations |
| `tree-walk-interpreter/docs/decisions/` | **Decision records** — why a non-obvious implementation choice was made |

## Development Principles

### Spec-First Development
- The language specification (`docs/public/spec.md`) is authoritative
- Implementation reveals spec ambiguities — resolve in the spec first, then implement
- Never implement behavior not specified in the spec
- The spec describes the language, not the interpreter's current state — implementation status is tracked by GitHub issues and the version milestone

### Task Management
- **GitHub Projects v2** (https://github.com/users/Vladastos/projects/1) is the source of truth for task status and planning
- Issues are the unit of work; the project board is the canonical status view
- Before creating a task, search first: `gh issue list --search "keyword"` to avoid duplicates
- Apply labels and a milestone when creating: `--label "evaluator" --milestone "v0.5.0"`
- Use `gh issue edit <number> --add-label "status:in-progress"` when starting a task — update the project Status field to **In Progress** as well
- **Task state changes require no commit** — the project board is the source of truth, not files in the repo
- **The main repo only gets a commit when actual code is written**

### Commit Message Convention
- All commits related to a task **must include the issue number** in the message
- Format: `<type>(#<issue>): <description>`
- Types: `feat`, `fix`, `refactor`, `test`, `docs`
- Example: `feat(#42): add generic type inference`
- **Closing commits must include a body**: a bullet list of what was done
  ```
  feat(#42): add generic type inference

  - Added unification for generic type variables in typeinference/mod.rs
  - Extended TypeEnv to track generic constraints
  - Added 12 integration tests covering polymorphic functions

  Closes #42
  ```
- Add `Closes #<number>` in the commit body to auto-close the issue on push

### Three-Stage Validation
1. **Designed**: Written in spec, not yet implemented
2. **Interpreter-validated**: Implemented and tested in tree-walk interpreter
3. **Compiler-validated**: Future compiler implementation (not current focus)

### Type System Stability
`src/typeinference/mod.rs` and `src/typechecker/mod.rs` are the most sensitive files in the codebase. Bugs here produce silent mis-compilations, not crashes, and are hard to detect through tests alone.

**Before committing any change to these files:**
- Run `/review-typechecker` and work through the full checklist
- Run `cargo test` — every test must pass, including unrelated ones
- See **AGENTS.md § Type System Stability** for invariants and patterns to preserve

## Key Source Files

- `src/typeinference/mod.rs`: Core type inference engine
- `src/typechecker/mod.rs`: Type checker validation pass
- `src/types/mod.rs`: Type representation
- `src/typed_ast/`: AST nodes with type annotations
- `src/evaluator/`: Tree-walking interpreter
- `tests/typeinference_tests.rs`: Phase-based type inference test suite
- `tests/typechecking/typechecking_tests.rs`: Typechecking integration tests

## Current Development Focus

The language is at **v0.4.2** (evaluator refactor complete). The next release is **v0.5.0** (Module System).

Check open tasks: `gh issue list --milestone "v0.5.0"`.

See [`docs/internal/versioning.md`](docs/internal/versioning.md) for the versioning model.

## Error Handling

Uses miette for rich error reporting with source context. Error types are defined in `src/error/mod.rs` with proper source location tracking.

## Dependencies

- **pest**: PEG parser generator (grammar in `src/grammar.pest`)
- **miette**: Rich error reporting with source context
- **thiserror**: Error derive macros
- **clap**: CLI argument parsing

## Testing Strategy

- Phase-based test development for type inference (`tests/typeinference_tests.rs`)
- Stage-based typechecking tests in `tests/typechecking/` with `.mln` source files
- Integration tests in `tests/parsing/` for parsing validation
- Unit tests within component modules