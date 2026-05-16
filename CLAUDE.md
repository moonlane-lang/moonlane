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

All docs, tasks, milestones, and decision records are managed by the **Backlog.md MCP server**. The data lives under `docs/backlog/`.

Use the Backlog.md MCP tools (or read `backlog://workflow/overview`) to browse and update tasks. Do **not** edit files under `docs/backlog/` directly.

### Key Docs (via Backlog.md MCP)

| Doc ID | Purpose |
|---|---|
| `doc-2` | Language Specification — single source of truth |
| `doc-3` | Spec Backlog — open design questions and deferred features |
| `doc-4` | Architecture Overview — pipeline diagram, component boundaries |
| `doc-5`, `doc-6`, `doc-7` | Type Inference — concepts, implementation guide, roadmap |

Decision records live in `docs/backlog/decisions/`. Milestones (epics and phases) are in `docs/backlog/milestones/`.

## Development Principles

### Spec-First Development
- The language specification (`doc-2` in Backlog.md MCP) is authoritative
- Implementation reveals spec ambiguities — resolve in the spec first, then implement
- Never implement behavior not specified in the spec
- Tag spec sections when interpreter-validated: `> ✓ Interpreter-validated (v0.1)`

### Task Management
- All tasks are managed via the **Backlog.md MCP server** — use its tools to create, update, and close tasks
- Read `backlog://workflow/overview` (or call `backlog.get_backlog_instructions()`) before creating tasks to avoid duplicates and follow the correct workflow
- Task statuses: `open`, `in-progress`, `done`, `blocked`
- Every task should link to the relevant spec doc or backlog item

### Three-Stage Validation
1. **Designed**: Written in spec, not yet implemented
2. **Interpreter-validated**: Implemented and tested in tree-walk interpreter
3. **Compiler-validated**: Future compiler implementation (not current focus)

## Key Source Files

- `src/typeinference/mod.rs`: Core type inference engine
- `src/typechecker/mod.rs`: Type checker validation pass
- `src/types/mod.rs`: Type representation
- `src/typed_ast/`: AST nodes with type annotations
- `src/evaluator/`: Tree-walking interpreter
- `tests/typeinference_tests.rs`: Phase-based type inference test suite
- `tests/typechecking/typechecking_tests.rs`: Typechecking integration tests

## Current Development Focus

Check the Backlog.md MCP for the current open tasks and milestone status. The active epics are:

- **Epic 001** (Typechecker and Typed AST) — largely done; stage 6 typechecking tasks in progress
- **Epic 002** (Evaluator) — expression and statement evaluation
- **Epic 003** (Generics and Monomorphization)
- **Epic 004** (Traits and Method Dispatch)
- **Epic 005** (Typechecker Integration)

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