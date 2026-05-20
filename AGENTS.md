# Yoloscript — Agent Guide

## Project

Yoloscript is a statically-typed, expression-oriented scripting language. This repository contains its interpreter (Phase 01 PoC). Tasks are tracked in GitHub Projects v2; spec docs and decision records live in `backlog/` (to be reorganised into `docs/` — see issue #19).

---

## Documentation Structure

| Location | Purpose |
|---|---|
| `backlog/docs/doc-2` | **Language Specification** — single source of truth for the language. If it's not here, it doesn't exist yet. |
| `backlog/docs/doc-3` | **Spec Backlog** — open design questions and deferred features |
| `backlog/docs/doc-4` | **Architecture Overview** — pipeline diagram, component boundaries |
| `backlog/docs/doc-5,6,7` | **Type Inference docs** — concepts, implementation guide, roadmap |
| `backlog/decisions/` | **Decision records** — why a non-obvious choice was made |
| GitHub Projects v2 | **Task board** — canonical status view (https://github.com/users/Vladastos/projects/1) |
| GitHub Issues | **Tasks** — unit of work; use `gh issue list` for CLI access |
| GitHub Milestones | **Milestones** — Epic 001–005 and Phase 01–03 |

---

## Task Workflow

### Before starting a task (open → in-progress)

1. **Read the full issue** including all acceptance criteria: `gh issue view <number>`
2. **Check the spec** — read every spec section the task touches. Identify anything ambiguous or missing.
   - If a spec gap exists: **STOP**. Fix the spec first (`backlog/docs/doc-2`). If the fix requires a non-obvious decision, write a decision record first.
3. **Check existing decisions** — `grep` or `ls` in `backlog/decisions/` for any ADR that governs the area being changed. Read it before writing any code.
4. **Check dependencies** — verify every linked issue is closed and its implementation matches what this task expects.
5. **If no clear path forward exists** — STOP. Ask for guidance before beginning implementation. Do not make a significant architectural decision unilaterally.
6. **Mark in-progress**: `gh issue edit <number> --add-label "status:in-progress"` and set the project Status field to **In Progress**

### During implementation

- **Follow the spec exactly.** If behaviour is not described in the spec, it does not exist. Add it to the spec before implementing it.
- **If an ambiguity surfaces mid-implementation**: stop, decide (write a decision record if non-obvious), update the spec, then continue. Never implement an undocumented behaviour and "fix the docs later."
- **If a spec section turns out to be wrong or impractical**: stop, write a decision record superseding the previous understanding, update the spec, then implement against the updated spec.
- **Do not expand scope.** If you discover necessary work outside the task boundary, open a new issue for it. Finish the current task first unless the out-of-scope work is a hard blocker.

### Before closing a task (in-progress → done)

1. All acceptance criteria must be checked off — no exceptions.
2. All tests must pass, including tests from earlier tasks.
3. If any non-obvious decisions were made during implementation → create a decision record.
4. If the implementation revealed spec gaps that you fixed → verify the spec edit is committed.
5. If a spec section is now interpreter-validated, tag it: `> ✓ Interpreter-validated (v0.1)`
6. **Close the issue**: `gh issue close <number>` (or include `Closes #<number>` in the commit body to auto-close on push). The project Status field updates to **Done** automatically, and the `status:in-progress` label is removed by CI.

### Opening a new issue

```bash
gh issue create \
  --title "Brief imperative title" \
  --label "evaluator" \
  --milestone "Epic 002 - Evaluator" \
  --body "## Description\n...\n\n## Acceptance Criteria\n- [ ] ..."
```

Search for duplicates first: `gh issue list --search "keyword"`

---

## Commit Convention

Every commit related to a task **must reference the issue number**:

```
<type>(#<number>): <description>
```

Types: `feat`, `fix`, `refactor`, `test`, `docs`. Commits unrelated to any issue omit the reference: `docs: fix typo in README`.

### One commit stream — main repo only

Task state changes happen on GitHub Issues, not in the repo. **The main repo only gets a commit when actual code or docs are written.**

### Commit reference table

| Situation | Type | Example |
|---|---|---|
| Code change for a task | `feat` / `fix` / `refactor` / `test` | `feat(#42): add generic type inference` |
| Spec or doc edit | `docs` | `docs(#42): clarify let-polymorphism in §4.2` |
| Decision record | `docs` | `docs: add decision — unify type var generation` |

### Closing commits require a body

```
feat(#42): add generic type inference

- Added unification for generic type variables in typeinference/mod.rs
- Extended TypeEnv to track generic constraints
- Added 12 integration tests covering polymorphic functions

Closes #42
```

`Closes #42` in the body auto-closes the issue when the commit lands on main.

---

## When to STOP and Ask

Stop and ask the user before proceeding when:

- **A design decision is required** with no clearly correct answer — multiple options exist and the choice has architectural consequences.
- **The spec is ambiguous** in a way that affects the implementation, and the right interpretation is not obvious.
- **Implementing would require changing things outside the task scope** in ways that could affect other tasks or break existing behaviour.
- **A dependency is incomplete or wrong** — the task assumes a contract that the dependency does not deliver.
- **The task description seems out of date** — it references things that no longer exist or contradict the current codebase state.
- **You are about to make an irreversible or difficult-to-reverse change** — schema changes, API breaks, deleted code.

When you stop, explain clearly: what you found, what the options are, and what you recommend. Do not just block — give the user enough context to make a decision.

---

## Decision Records

Create a decision record (a new `.md` file in `backlog/decisions/`, following the naming and format of existing records) when:

- Multiple reasonable implementation options existed and the choice was non-trivial.
- The rationale will matter when revisiting this area later.
- A spec section is being changed due to an implementation finding.
- A previous decision is being reversed.

Do **not** create a decision record for:

- Choices with an obvious single answer.
- Routine implementation details that follow directly from the spec.
- Things already covered by an existing decision record.

Accepted decisions are never modified. To reverse one, create a new decision record that supersedes the old one and update its status field.

---

## Spec Discipline

- The spec is the source of truth. Implementation follows the spec; the spec does not follow the implementation.
- The spec does not contain rationale, history, or open questions. Those belong in decision records and the spec backlog respectively.
- When a backlog item is resolved: remove it from `doc-3` (Spec Backlog) and write it into `doc-2` (Language Spec).
- Do not skip validation levels: interpreter validates before compiler implements.

---

## Type System Stability

The type inference (`src/typeinference/mod.rs`) and typechecker (`src/typechecker/mod.rs`) are the most sensitive components in the codebase. Bugs here produce silent mis-compilations — not crashes — and are hard to detect through tests alone. Treat changes to these files with more care than anything else.

### Two-pass architecture invariants

The typechecker runs in two passes that must remain strictly separated:

- **Pass 1 (inference)**: Walks the AST, pushes constraints, solves into `ctx.subst`. Side-effects only on `ctx`.
- **Pass 2 (construct)**: Walks the AST again, reads `ctx.subst`, builds `TypedAST`. Must not infer or constrain — only resolve.

Do not infer types in Pass 2. Do not build TypedAST nodes in Pass 1. If you find yourself doing either, stop and ask.

### Key invariants to preserve

- **`Substitution::compose` is ordered**: `a.compose(b)` means "apply `b` to `a`'s values, then merge" — equivalent to `a ∘ b` (b first). Reversing the arguments changes the semantics. Always check which direction is correct at each call site.
- **`Never` is a bottom type**: `unify(Never, T)` always succeeds. This means typechecking tests cannot distinguish a `Never`-typed expression from a correctly typed one. Use evaluator tests (once available) to verify runtime correctness.
- **`type_to_infer` normalises `Perhaps`/`Result`**: These are distinct `Type` variants but normalise to `InferType::Named`. Code that pattern-matches on `Type::Named` will miss them unless routed through `type_to_infer` first.
- **`TypeVar` vs `InferType::Var`**: Formal type parameters in `EnumInfo`/`StructInfo` are stored as `TypeVar`. Fresh variables at a usage site are `InferType::Var(TypeVar)`. Do not confuse the two — passing a formal `TypeVar` where a fresh `InferType::Var` is expected silently produces wrong substitutions.
- **`instantiate_scheme_for_call` is the canonical pattern** for generic instantiation: create fresh `InferType::Var` per type param, build `init_subst`, unify instantiated types against actuals, then extract concrete types from the composed substitution. Replicate this pattern; do not invent alternatives.

### Before committing changes to these files

1. Run the **full test suite**: `cargo test` from `tree-walk-interpreter/`. Every test must pass — regressions in unrelated tests are a signal that a shared invariant was broken.
2. Run `/review-typechecker` and work through the checklist before finalising.
3. If you added a new `unify` call: verify the argument order is `(expected, actual)` and that substitution composition is in the correct direction.
4. If you added a new `infer_type_to_type` call: verify the call site has access to a `Span` and that all `InferType::Var` cases are resolved before the call.
5. If you changed `construct_block`'s signature or the threading of `expected_ty`: verify every call site passes the correct expected type (function return type, annotation type, or `None`) — a `None` where a type is expected causes annotation-dependent failures.

### When to STOP on type system changes

- A change requires touching both `mod.rs` files simultaneously — this is a sign the boundary between passes is being violated.
- You cannot find an existing pattern (in `instantiate_scheme_for_call`, `construct_expr`, etc.) that covers the new case — ask before inventing.
- A test that was passing begins failing after a substitution composition change — the ordering bug may affect other cases not covered by tests.

---

## What Not to Do

- Do not implement behaviour that is not in the spec.
- Do not let implementation diverge from the spec and fix the docs later.
- Do not add rationale or history to the spec — that belongs in a decision record.
- Do not create new tracking documents — all open work goes into GitHub Issues and is tracked on the project board.
- Do not start implementation if the task description has unresolved questions.
- Do not mark a task done with unchecked acceptance criteria.
- Do not make significant architectural decisions alone — ask first.

