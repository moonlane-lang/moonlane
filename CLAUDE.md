# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

Yoloscript is a Rust-inspired programming language with a tree-walk interpreter written in Rust. The project implements a statically typed, expression-oriented language with features like type inference, pattern matching, and generics.

## Common Development Commands

### Building and Running
```bash
# Build the interpreter
cd tree-walk-interpreter
cargo build --release

# Run a Yoloscript program
cargo run -- path/to/program.yolo

# Run in debug mode with output
cargo run -- --debug path/to/program.yolo
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
.yolo source → Parser (pest) → AST Builder → Type Checker → Evaluator
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
gh issue list --milestone "Epic 002 - Evaluator"       # filter by milestone
gh issue view <number>                                 # read a task
gh issue create --title "..." --label "..." --milestone "..."  # create a task
gh issue close <number>                                # mark done
gh issue comment <number> --body "..."                 # add a note
gh issue edit <number> --add-label "status:in-progress"        # mark in-progress
```

**Labels:** `evaluator`, `generics`, `traits`, `integration`, `tooling`, `dx`, `migration`, `docs`, `typechecker`, `type-inference`, `architecture`, `priority:low/medium/high`, `status:backlog`, `status:in-progress`, `archived`

**Milestones:** Epic 001–005 and Phase 01–03, matching the original backlog milestone structure.

### Docs and Decisions — backlog/ submodule

Spec documents and decision records live in `backlog/` (to be reorganised into `docs/` in #19). Read them directly — no MCP tooling needed.

| Path | Purpose |
|---|---|
| `backlog/docs/doc-2` | **Language Specification** — single source of truth |
| `backlog/docs/doc-3` | **Spec Backlog** — open design questions and deferred features |
| `backlog/docs/doc-4` | **Architecture Overview** — pipeline diagram, component boundaries |
| `backlog/docs/doc-5`, `doc-6`, `doc-7` | **Type Inference** — concepts, implementation guide, roadmap |
| `backlog/decisions/` | **Decision records** — why a non-obvious choice was made |

## Development Principles

### Spec-First Development
- The language specification (`backlog/docs/doc-2`) is authoritative
- Implementation reveals spec ambiguities — resolve in the spec first, then implement
- Never implement behavior not specified in the spec
- Tag spec sections when interpreter-validated: `> ✓ Interpreter-validated (v0.1)`

### Task Management
- **GitHub Projects v2** (https://github.com/users/Vladastos/projects/1) is the source of truth for task status and planning
- Issues are the unit of work; the project board is the canonical status view
- Before creating a task, search first: `gh issue list --search "keyword"` to avoid duplicates
- Apply labels and a milestone when creating: `--label "evaluator" --milestone "Epic 002 - Evaluator"`
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

Check GitHub Issues for open tasks: `gh issue list --milestone "Epic 002 - Evaluator"`. The active epics are:

- **Epic 001** (Typechecker and Typed AST) — complete
- **Epic 002** (Evaluator) — next up; issues #1–#4
- **Epic 003** (Generics and Monomorphization) — issues #5–#10
- **Epic 004** (Traits and Method Dispatch) — issues #11–#13
- **Epic 005** (Typechecker Integration) — issue #14

## Error Handling

Uses miette for rich error reporting with source context. Error types are defined in `src/error/mod.rs` with proper source location tracking.

## Dependencies

- **pest**: PEG parser generator (grammar in `src/grammar.pest`)
- **miette**: Rich error reporting with source context
- **thiserror**: Error derive macros
- **clap**: CLI argument parsing

## Testing Strategy

- Phase-based test development for type inference (`tests/typeinference_tests.rs`)
- Stage-based typechecking tests in `tests/typechecking/` with `.yolo` source files
- Integration tests in `tests/parsing/` for parsing validation
- Unit tests within component modules