# /sprint-end

Close a sprint: run the mandatory quality gate, then build and publish the sprint review issue and PR.

**Arguments:** `$ARGUMENTS` — sprint number, e.g. `1`

**This skill will not produce a PR until every quality gate passes. See AGENTS.md § Quality Gate for the rationale.**

---

## Step 1 — Fetch sprint context

Read the kickoff issue to get the sprint goal, planned issue list, and milestone:
```bash
gh issue list --repo moonlane-lang/moonlane --search "Sprint $ARGUMENTS Kickoff" --state all --json number,title,body,milestone
```

Identify the **milestone** (e.g. `v0.3`) from the kickoff issue's milestone field — every issue created during sprint-end must use this same milestone.

Categorise all planned issues as completed (closed) or carried over (still open):
```bash
gh issue list --repo moonlane-lang/moonlane --milestone "<milestone>" --state open --json number,title
```

---

## Step 2 — Quality Gate (mandatory — do not skip)

Work through every gate in order. **If any gate fails, stop, report what failed, and do not proceed to the PR step.**

### Gate 1: Test suite

```bash
cd moonlane-interpreter && cargo test
```

All tests must pass with zero failures. If any fail, fix them before continuing.

### Gate 2: Code quality

Inspect every file changed on the sprint branch relative to `main`:
```bash
git diff main..HEAD --name-only
```

For each changed Rust file, check:
- No stale `todo!()`, `unimplemented!()`, or `unreachable!()` without a tracking issue linked in a comment.
- No `unwrap()` or `expect()` on paths that can fail at runtime (parsing errors, env lookups on user input).
- No unused imports, dead match arms, or commented-out code left behind.
- Builtins are registered in **all** required places: `src/typechecker/registry.rs` (inference pass) **and** `src/typechecker/construction.rs` (construction pass). A builtin missing from construction.rs will typecheck but fail with "undefined name" at runtime.

Report each finding. If a `todo!()`/`unreachable!()` is intentional (e.g. a placeholder variant per an RFC), verify a tracking issue exists and note it.

### Gate 3: Test coverage

For every feature or fix introduced in the sprint, verify a test exists:

| Change type | Required test location |
|---|---|
| New builtin | `moonlane-interpreter/tests/typechecking/sources/stage*_*.mln` — positive and at least one negative (wrong arg type) |
| New grammar construct | Parsing test or typechecking test |
| New evaluator behaviour | Evaluator test in `moonlane-interpreter/tests/evaluator_tests.rs` or integration `.mln` file |
| Bug fix | A regression test that would have caught the original bug |
| New error code | A negative typechecking or evaluator test that triggers it |

List each sprint change and confirm its test. Flag any untested changes — either add a test or document in the review issue why it is untestable.

### Gate 4: Spec accuracy

For every language-visible change in the sprint, verify the spec reflects it:

1. Check `docs/public/spec/runtime.md` builtin table against `src/typechecker/registry.rs` — every `ctx.bind_poly(...)` call must have a matching row.
2. Check `docs/public/spec/` for each new grammar construct — it must be described in the appropriate section (expressions, declarations, etc.).
3. Check each RFC implemented this sprint — its frontmatter `status` must be `incorporated`.
4. Check `docs/public/changelog.md` — the current version milestone must have an entry listing the sprint's shipped features.

Report any spec/code divergence found.

### Gate 5: Spec completeness

Read `docs/public/spec.md` and every section it links to. Verify:
- No section refers to a feature that was removed or renamed this sprint without updating the reference.
- No `TODO`, `TBD`, or `(coming soon)` markers were introduced by sprint work.
- All cross-references between spec sections are still valid.

### Gate 6: Internal doc accuracy

For every component touched during the sprint, check:

| Component | Doc to verify |
|---|---|
| Evaluator (`src/evaluator/`) | `moonlane-interpreter/docs/evaluator.md` — Value variants, signals, builtins, known limitations |
| Typechecker (`src/typechecker/`) | `moonlane-interpreter/docs/typechecker.md` — passes, constraints, inference rules |
| Parser / grammar | `moonlane-interpreter/docs/architecture.md` — pipeline diagram still accurate |

Report any internal doc that is stale or missing.

### Gate 7: Architectural decision records

Review every commit on the sprint branch:
```bash
git log main..HEAD --oneline
```

For each commit, ask: did this change involve a non-obvious architectural decision? Use the criteria from AGENTS.md § Decision Records. Examples of what qualifies:
- A choice between two plausible designs with real trade-offs (e.g. normalising in the parser vs. the type checker)
- A deliberate deviation from a prior decision record or RFC
- A constraint or invariant that future contributors must know to avoid breaking the design
- A workaround for a language or library limitation that isn't obvious from the code

For each qualifying decision, verify a decision record exists in `moonlane-interpreter/docs/decisions/`. If any are missing, create them now — before the PR is opened.

List every qualifying decision found and whether a record exists or was created.

---

## Step 3 — Fix findings before continuing

For every failing gate from Step 2: fix the issue, commit to the sprint branch, and re-run the relevant gate check. Do not proceed until all gates are green.

---

## Step 4 — Integration tests (language version sprints only)

**If this sprint ships a language version milestone** (i.e. the milestone is a version tag such as `v0.3`):

Write comprehensive integration tests that exercise the **complete feature set** of that version — not just features added this sprint. These tests must:
- Live in `moonlane-interpreter/tests/evaluator/sources/` as `int_NN_<name>.mln`
- Be self-asserting (`assert(...)`)
- Cover all combinations of new features interacting (generics + closures, structs + enums, etc.)
- Use idiomatic Moonlane: type annotations where expected, explicit braces where required

After writing the tests, run them:
```bash
cargo test int_
```

**Examine every failure and inconsistency found.** For each:
- If it is a bug: fix it, add a regression test, commit.
- If it exposes a spec ambiguity: note it and open a tracking issue.
- If it reveals a limitation that is out of scope for this version: document it in the relevant `docs/*.md` Known Limitations section and open a tracking issue for the next version.

Report a summary of: tests written, failures found, fixes made, issues opened.

Do not proceed to Step 5 until all integration tests pass.

---

## Step 5 — Bump the crate version in Cargo.toml

Read the milestone version from the kickoff issue (e.g. `v0.4.2`). Strip the leading `v` to get the semver string (e.g. `0.4.2`).

Open `moonlane-interpreter/Cargo.toml` and update the `version` field to match:
```toml
version = "0.4.2"
```

Commit the change on the sprint branch:
```bash
git add moonlane-interpreter/Cargo.toml
git commit -m "chore(#<kickoff-issue-number>): bump crate version to <version>"
```

The crate version must match the milestone before the PR is opened.

---

## Step 6 — Move carried-over issues to backlog

For each issue that is still open and was planned for this sprint:
```bash
gh issue edit <N> --repo moonlane-lang/moonlane \
  --remove-label "status:in-progress" \
  --add-label "status:backlog"
```

---

## Step 7 — Update the kickoff issue

Edit the kickoff issue body to reflect what was actually completed vs. deferred (use checkboxes: `[x]` for done, `[ ]` for carried over). This is the factual record of the sprint.

---

## Step 8 — Gather epic and spec notes

Epic progress — for each milestone touched this sprint:
```bash
gh api repos/moonlane-lang/moonlane/milestones --jq '.[] | select(.title == "<milestone>") | "\(.title): \(.closed_issues)/\(.open_issues + .closed_issues) issues closed"'
```

Spec changes — all commits on the sprint branch that touched `docs/`:
```bash
git log main..HEAD --oneline -- docs/
```

---

## Step 9 — Create the sprint review issue

Use the **same milestone** as the kickoff issue:

```bash
gh issue create \
  --repo moonlane-lang/moonlane \
  --title "Sprint $ARGUMENTS Review" \
  --milestone "<milestone>" \
  --label "sprint:review" \
  --body "$(cat <<'EOF'
## Sprint Goal
<goal from kickoff issue>

## Quality Gate Results
All gates passed. ✅

### Code quality findings resolved
<list any findings from Gate 2 that were fixed, or "None">

### Test coverage gaps resolved
<list any untested changes that had tests added, or "None">

### Spec / doc fixes
<list any spec or doc corrections made during gate checks, or "None">

### Integration tests (if version sprint)
<summary: N tests written, M failures found, K fixed, issues opened — or "N/A">

## Completed
<- [x] #N Title for each closed issue>

## Carried Over
<- [ ] #N Title for each open issue, with reason>

## Epic Progress
<milestone progress lines>

## Spec Notes
<doc commit summaries, or "No spec changes this sprint.">

## Next Sprint Seeds
<!-- Add ideas for the next sprint here -->
EOF
)"
```

Note the issue number returned — needed for the PR.

---

## Step 10 — Open the pull request

```bash
gh pr create \
  --repo moonlane-lang/moonlane \
  --base main \
  --head sprint/$ARGUMENTS \
  --title "Sprint $ARGUMENTS — <theme>" \
  --body "$(cat <<'EOF'
Sprint review: #<review-issue-number>

Closes #<review-issue-number>
Closes #<kickoff-issue-number>
EOF
)"
```

Both `Closes` lines are required — on merge, GitHub automatically closes both the review issue and the kickoff issue.

---

## Step 11 — Hand off to user

> **Sprint $ARGUMENTS quality gate passed and PR is open.**
>
> - All 7 quality gates: ✅
> - Review issue: #<N> — add **Next Sprint Seeds** if you have ideas.
> - **Merge the PR** on GitHub — this automatically closes the review and kickoff issues.
> - After the merge is confirmed, pull `main` and create the release tag:
>   ```bash
>   git pull origin main
>   git tag -a v<X.Y> -m "v<X.Y>: <sprint theme>" && git push origin v<X.Y>
>   ```
> - Delete the `sprint/$ARGUMENTS` branch on GitHub.

**The tag must be created on `main` after the PR is merged — never on the sprint branch, and never before the merge.**
The tag name must match the version in `docs/public/changelog.md`.

---

## Notes

- Do not create the PR until every quality gate in Step 2 passes. This is enforced by the skill structure — Step 3 must be completed before Step 10.
- Do not create the release tag — instruct the user to create it after merging. The tag must point to `main`.
- A sprint with 0 completed issues still produces a review issue — record why in Completed.
- If spec ambiguities surfaced (visible in Gate 5 or Spec Notes), prompt the user to open a `/new-rfc`.
- The sprint branch must not be deleted until after the PR is merged.
- All issues created during sprint-end (review issue, any tracking issues) must carry the sprint's milestone.
