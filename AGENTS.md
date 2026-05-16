# Yoloscript — Agent Guide

## Project

Yoloscript is a statically-typed, expression-oriented scripting language. This repository contains its interpreter (Phase 01 PoC). The language spec, architecture docs, task backlog, and decision records all live inside `backlog/` and `yoloscript-docs/`.

---

## Documentation Structure

| Location | Purpose |
|---|---|
| `backlog/docs/doc-2` | **Language Specification** — single source of truth for the language. If it's not here, it doesn't exist yet. |
| `backlog/docs/doc-3` | **Spec Backlog** — open design questions and deferred features |
| `backlog/docs/doc-4` | **Architecture Overview** — pipeline diagram, component boundaries |
| `backlog/docs/doc-5,6,7` | **Type Inference docs** — concepts, implementation guide, roadmap |
| `backlog/decisions/` | **Decision records** — why a non-obvious choice was made |
| `backlog/milestones/` | **Milestones** — epics (m-4 to m-8) and phases (m-1 to m-3) |
| `backlog/tasks/` | **Tasks** — all work items |

---

## Task Workflow

### Before starting a task (To Do → In Progress)

1. **Read the full task description** including all acceptance criteria and the "What" section.
2. **Check the spec** — read every spec section the task touches. Identify anything ambiguous or missing.
   - If a spec gap exists: **STOP**. Fix the spec first (edit `yoloscript-docs/01-SPEC/LANGUAGE-SPEC.md`). If the fix requires a non-obvious decision, write a decision record first.
3. **Check existing decisions** — search `backlog/decisions/` for any ADR that governs the area being changed. Read it before writing any code.
4. **Check dependencies** — verify every listed dependency task is actually done and its implementation matches what this task expects.
5. **If no clear path forward exists** — STOP. Ask for guidance before beginning implementation. Do not make a significant architectural decision unilaterally.

### During implementation

- **Follow the spec exactly.** If behaviour is not described in the spec, it does not exist. Add it to the spec before implementing it.
- **If an ambiguity surfaces mid-implementation**: stop, decide (write a decision record if non-obvious), update the spec, then continue. Never implement an undocumented behaviour and "fix the docs later."
- **If a spec section turns out to be wrong or impractical**: stop, write a decision record superseding the previous understanding, update the spec, then implement against the updated spec.
- **Do not expand scope.** If you discover necessary work outside the task boundary, create a new task for it. Finish the current task first unless the out-of-scope work is a hard blocker.

### Before marking a task done (In Progress → Done)

1. All acceptance criteria must be checked off — no exceptions.
2. All tests must pass, including tests from earlier tasks.
3. If any non-obvious decisions were made during implementation → create a decision record.
4. If the implementation revealed spec gaps that you fixed → verify the spec edit is committed.
5. If a spec section is now interpreter-validated, tag it: `> ✓ Interpreter-validated (v0.1)`

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

Create a decision record (`backlog decision create`) when:

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

## What Not to Do

- Do not implement behaviour that is not in the spec.
- Do not let implementation diverge from the spec and fix the docs later.
- Do not add rationale or history to the spec — that belongs in a decision record.
- Do not create new tracking documents — all open work goes into the backlog.
- Do not start implementation if the task description has unresolved questions.
- Do not mark a task done with unchecked acceptance criteria.
- Do not make significant architectural decisions alone — ask first.

---

<!-- BACKLOG.MD MCP GUIDELINES START -->

<CRITICAL_INSTRUCTION>

## BACKLOG WORKFLOW INSTRUCTIONS

This project uses Backlog.md MCP for all task and project management activities.

**CRITICAL GUIDANCE**

- If your client supports MCP resources, read `backlog://workflow/overview` to understand when and how to use Backlog for this project.
- If your client only supports tools or the above request fails, call `backlog.get_backlog_instructions()` to load the tool-oriented overview. Use the `instruction` selector when you need `task-creation`, `task-execution`, or `task-finalization`.

- **First time working here?** Read the overview resource IMMEDIATELY to learn the workflow
- **Already familiar?** You should have the overview cached ("## Backlog.md Overview (MCP)")
- **When to read it**: BEFORE creating tasks, or when you're unsure whether to track work

These guides cover:
- Decision framework for when to create tasks
- Search-first workflow to avoid duplicates
- Links to detailed guides for task creation, execution, and finalization
- MCP tools reference

You MUST read the overview resource to understand the complete workflow. The information is NOT summarized here.

</CRITICAL_INSTRUCTION>

<!-- BACKLOG.MD MCP GUIDELINES END -->
