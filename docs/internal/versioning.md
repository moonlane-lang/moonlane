---
id: versioning
title: "Versioning Model"
type: guide
created_date: '2026-05-21'
---

# Moonlane Versioning Model

This document is the authority on version numbering, the RFC lifecycle, and documentation conventions. All other guides defer to it on these topics.

---

## Language Version Numbering

Language versions follow `v<major>.<minor>`:

- **Minor bump** (`v0.1 → v0.2`): backward-compatible additions — new syntax, new builtins, or features incorporated from accepted RFCs.
- **Major bump** (`v0.x → v1.0`, `v1.x → v2.0`): breaking changes to the syntax or semantics of existing programs.

No patch level exists for the language itself. Spec clarifications that do not change behaviour are committed to the living spec document and noted in the CHANGELOG without incrementing the version.

### Pre-1.0 era

Versions before `v1.0` cover the period during which both the interpreter and the compiler are reaching production quality. The language is actively evolving; minor versions may introduce significant new capabilities (generics, traits, concurrency, the memory model). Breaking changes before `v1.0` are possible but must be explicitly called out in the CHANGELOG.

The interpreter is the first backend to reach each language version. The compiler follows, targeting the same spec version. Both backends are permanent and supported — the interpreter is not a prototype to be discarded when the compiler exists. See `docs/internal/vision.md` for the full dual-mode commitment.

---

## Backend Version Numbering

Both the interpreter and the compiler follow `v<major>.<minor>.<patch>`:

- `major.minor` always matches the spec version the backend fully implements.
- `patch` increments for bug fixes that do not change the implemented language.

`interpreter v0.2.3` means: implements spec v0.2, third patch release.  
`compiler v0.3.0` means: implements spec v0.3, first release.

Backends are versioned independently — the interpreter may be at `v0.3.x` while the compiler is still at `v0.2.x` if it lags behind. The spec version is the shared reference point.

### Backend milestone targets

| Spec version | Interpreter | Compiler | Notes |
|---|---|---|---|
| v0.1 | shipped | — | Interpreter only; compiler not yet started |
| v0.2 | target | — | Generics and traits |
| v0.3 | target | — | Memory model (linear types, pointers, closure capture) |
| v0.4 | target | target (v0.3 subset) | Concurrency; compiler begins targeting v0.3 |
| v0.5 | target | target | Attributes, macros, derive; both backends in sync |
| v1.0 | target | target | Both backends production-quality; first stable release |

The compiler is not required to track the interpreter version-for-version before v1.0. It may lag by one minor version. At v1.0, both backends must implement the full spec.

---

## The Spec as a Living Document

`docs/public/spec.md` is the entry point for the language specification. It links to focused sub-files in `docs/public/spec/`. The spec describes the full language including features planned for future versions. Version snapshots are captured as **git tags**, not separate document files.

### Version tags

When a spec version is released, git tags are applied:

| Tag | Meaning |
|---|---|
| `spec-vX.Y` | Spec snapshot at this version |
| `interpreter-vX.Y.0` | First interpreter release for this spec version |
| `compiler-vX.Y.0` | First compiler release for this spec version (when applicable) |

**A tagged spec version is immutable.** If a spec error is discovered after tagging, it is documented as errata in the next version's CHANGELOG. Tags are never amended.

### Annotation style

Spec sections are annotated to indicate which version introduced or changed a feature:

| Situation | Annotation |
|---|---|
| Feature added in a specific version | `> *Since vX.Y.*` |
| Existing feature changed in a version | `> *Changed in vX.Y: description.*` |
| Feature planned for a future version | `> **vX.Y feature.** description...` |

The existing `> **v0.1:**` and `> **v0.2 feature.**` callouts in the spec conform to this convention.

---

## RFC Lifecycle

RFCs are the mechanism for proposing language changes. An RFC must be accepted and assigned a target version before implementation begins.

### States

| State | Meaning |
|---|---|
| `draft` | Being written; not yet ready for review |
| `under-review` | Ready for evaluation; set manually by the author |
| `accepted` | Design decided; `target: vX.Y` assigned; implementation may begin |
| `rejected` | Will not be implemented; reason recorded in `## Decision` |
| `deferred` | Not rejected, but not scheduled for any version |
| `incorporated` | Implemented and shipped in the target version |

### Frontmatter fields

```yaml
---
id: rfc-NNNN
title: "..."
date: 'YYYY-MM-DD'
status: draft          # one of the states above
---
```

The target version is **not** stored in the RFC frontmatter. It lives in exactly one place: the GitHub issue milestone. The `## Decision` section records it in prose (`**Target:** vX.Y`) as a human-readable audit trail, but the milestone is the authoritative field.

### Acceptance process

1. Author sets `status: under-review` when the RFC is ready for evaluation.
2. Discussion happens in the linked GitHub issue.
3. The project owner records the outcome in a `## Decision` section at the bottom of the RFC file.
4. **If accepted**: set `status: accepted`, assign the RFC's GitHub issue to the target version milestone, and record `**Target:** vX.Y` in the `## Decision` section.
5. **If rejected or deferred**: set status accordingly; record the reason in `## Decision`.

Once the RFC's target version ships (git tag applied), set `status: incorporated`.

### Decision section format

```markdown
## Decision

**Outcome:** Accepted / Rejected / Deferred  
**Target:** vX.Y *(if accepted)*

Brief rationale — why this design was chosen (or not), what alternatives were considered, and any constraints that drove the decision.
```

---

## GitHub Milestone Structure

| Milestone type | Examples | Purpose |
|---|---|---|
| **Version** | `v0.2`, `v0.3`, `v1.0` | Release planning — what ships in which version |
| **Epic** | `Epic 003 - Generics` | Implementation grouping — related issues within an area |

Implementation issues are assigned to the **version milestone** they target. Use label-based filtering (`--label "generics"`) for epic-level CLI queries — it works regardless of which milestone an issue is in.

---

## Changelog

Version entries live in `docs/public/changelog.md`. Each entry lists RFCs incorporated, features added, breaking changes (if any), and backend status (which backends implement the version).

---

## References

- Project vision and dual-mode commitment: `docs/internal/vision.md`
- Language spec: `docs/public/spec.md`
- Changelog: `docs/public/changelog.md`
