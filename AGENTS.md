# Gust — Agent Guide

## Project

Gust is a statically-typed, expression-oriented language. This repository contains its tree-walk interpreter. Tasks are tracked in GitHub Projects v2; spec docs and decision records live in `docs/`. The versioning model (language versions, RFC lifecycle, doc conventions) is defined in [`docs/internal/versioning.md`](docs/internal/versioning.md).

---

## Documentation Structure

| Location | Purpose |
|---|---|
| `docs/public/spec.md` | **Language Specification** — entry point; links to all spec sections. If it's not here, it doesn't exist yet. |
| `docs/public/spec/` | **Spec sections** — lexical, types, declarations, functions, expressions, runtime, grammar |
| `docs/public/changelog.md` | **Changelog** — per-version feature list |
| `docs/internal/rfcs/` | **RFCs** — language change proposals; see versioning model for lifecycle |
| `docs/internal/versioning.md` | **Versioning model** — version numbering, RFC lifecycle, doc conventions |
| `tree-walk-interpreter/docs/architecture.md` | **Architecture Overview** — pipeline diagram, component boundaries |
| `tree-walk-interpreter/docs/typechecker.md` | **Typechecker** — HM theory background + implementation notes |
| `tree-walk-interpreter/docs/evaluator.md` | **Evaluator** — runtime values, signals, environment, known limitations |
| `tree-walk-interpreter/docs/decisions/` | **Decision records** — why a non-obvious implementation choice was made |
| GitHub Projects v2 | **Task board** — canonical status view (https://github.com/orgs/gust-lang/projects/1) |
| GitHub Issues | **Tasks** — unit of work; use `gh issue list` for CLI access |
| GitHub Milestones | **Version milestones** (`v0.2`, `v0.3`, …) and **Epic milestones** (implementation groupings) |

---

## Sprint Workflow

Sprints are the unit of shipping. All sprint work must live on a dedicated branch and be merged into `main` via a pull request before the sprint is closed. The PR diff is the canonical sprint deliverable — it replaces any need to reconstruct what changed from individual commits.

### Starting a sprint

1. **Create the sprint branch** from the current `main`:
   ```bash
   git checkout main && git pull
   git checkout -b sprint/N        # e.g. sprint/3
   git push -u origin sprint/N
   ```
   Branch naming convention: `sprint/<N>` where `N` is the sprint number (matches the kickoff issue number's sprint label).

2. **Create the kickoff issue** (title: `Sprint N Kickoff — <theme>`) listing all planned tracks and issues. Leave it open until the sprint closes.

3. **Add all planned issues to the project board** with Status **Todo**. "In Progress" is reserved for tasks actively being worked on — sprint kickoff only moves issues from Backlog → Todo, never to In Progress.

3. **All subsequent work on this sprint goes on `sprint/N`**. This includes code commits, doc commits, and submodule pointer updates.

### During a sprint

- Every commit must be on `sprint/N`. Do not commit sprint work directly to `main`.
- Individual issue work follows the normal task workflow (see below).
- The sprint branch is pushed to origin after each logical unit of work (issue closed, fix applied, etc.).

### Closing a sprint

1. **Ensure all planned issues are closed** and all tests pass on the branch.
2. **Create the sprint review issue** (title: `Sprint N Review — <theme>`) summarising what was delivered, any debt carried forward, and architectural notes. Link it to the kickoff issue.
3. **Open a PR** from `sprint/N` → `main`:
   ```bash
   gh pr create \
     --base main \
     --head sprint/N \
     --title "Sprint N — <theme>" \
     --body "Sprint review: #<review-issue-number>\n\nCloses #<kickoff-issue-number>"
   ```
   The PR description must link the sprint review issue. The PR diff is the authoritative record of all changes made during the sprint.
4. **Merge the PR** (squash or merge commit — no force-push). Close the kickoff issue if not auto-closed.
5. **Delete the sprint branch** after merge.

### Why this process

- The PR diff makes sprint reviews straightforward: reviewers see exactly what changed, in what files, without reconstructing it from individual commits.
- `main` always reflects a completed, reviewed sprint — never mid-sprint state.
- Rollback of a sprint is a single revert if needed.

---

## Task Workflow

### Before starting a task (open → in-progress)

1. **Read the full issue** including all acceptance criteria: `gh issue view <number>`
2. **Check the spec** — read every spec section the task touches. Identify anything ambiguous or missing.
   - If a spec gap exists: **STOP**. Fix the spec first (`docs/public/spec.md`). If the fix requires a non-obvious decision, write a decision record first.
3. **Check existing decisions** — `grep` or `ls` in `tree-walk-interpreter/docs/decisions/` for any ADR that governs the area being changed. Read it before writing any code.
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
5. **Close the issue**: `gh issue close <number>` (or include `Closes #<number>` in the commit body to auto-close on push). The project Status field updates to **Done** automatically, and the `status:in-progress` label is removed by CI.

### Opening a new issue

```bash
gh issue create \
  --title "Brief imperative title" \
  --label "generics" \
  --milestone "v0.2" \
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

### One commit stream — sprint branch only

Task state changes happen on GitHub Issues, not in the repo. **The main repo only gets a commit when actual code or docs are written.** During an active sprint, all commits go on the sprint branch (`sprint/N`). Nothing is committed directly to `main` while a sprint is in progress.

### Commit reference table

| Situation | Type | Example |
|---|---|---|
| Code change for a task | `feat` / `fix` / `refactor` / `test` | `feat(#42): add generic type inference` |
| Spec or doc edit | `docs` | `docs(#42): clarify let-polymorphism in spec/declarations.md` |
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

Create a decision record (a new `.md` file in `tree-walk-interpreter/docs/decisions/`, following the naming and format of existing records) when:

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
- When an RFC is accepted and implemented, mark it `incorporated` in `docs/internal/rfcs/` and write the feature into the appropriate `docs/public/spec/` file.
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

## RFC Workflow

RFCs are design proposals for language changes. The full lifecycle is defined in [`docs/internal/versioning.md`](docs/internal/versioning.md).

### When to read an RFC

Before implementing any feature that has an associated RFC, read it. If the RFC `status` is `accepted`, implementation may proceed against its `## Proposal`. If `status` is `under-review` or `draft`, **stop** — the design is not settled; ask before implementing.

### During implementation of an accepted RFC

- Treat the RFC's `## Proposal` section as specification — the same discipline applies as with `docs/public/spec.md`.
- Any deviation from the proposal requires updating the RFC and writing a decision record explaining why.

### After the target version ships

Set `status: incorporated` in the RFC frontmatter.

---

## What Not to Do

- Do not implement behaviour that is not in the spec.
- Do not let implementation diverge from the spec and fix the docs later.
- Do not add rationale or history to the spec — that belongs in a decision record.
- Do not create new tracking documents — all open work goes into GitHub Issues and is tracked on the project board.
- Do not start implementation if the task description has unresolved questions.
- Do not mark a task done with unchecked acceptance criteria.
- Do not make significant architectural decisions alone — ask first.

