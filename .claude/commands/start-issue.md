# /start-issue

Begin work on a GitHub issue: read its details, mark it in-progress, and surface any dependencies.

**Arguments:** `$ARGUMENTS` — issue number, e.g. `4`

## Steps

1. **Verify the sprint branch:**
```bash
git branch --show-current
```
If the current branch is not `sprint/<N>`, warn the user and do not proceed. All sprint work must be committed to the active sprint branch, never directly to `main`.

2. **Read the issue:**
```bash
gh issue view <N> --repo moonlane-lang/moonlane
```
Display the title, description, acceptance criteria, labels, and milestone to the user.

3. **Check for dependencies** mentioned in the issue body (look for `#N` references or "Depends on" sections). If any dependency issues are still open, warn the user before proceeding.

4. **Mark as in-progress:**
```bash
gh issue edit <N> --repo moonlane-lang/moonlane \
  --remove-label "status:backlog" \
  --add-label "status:in-progress"
```

5. **Update the GitHub Projects v2 board** status to "In Progress" via GraphQL:
   - Fetch the project item ID for this issue
   - Set the Status field to the "In Progress" option

6. **Read relevant source and doc files** mentioned in the issue or inferable from its labels:
   - `evaluator` → `tree-walk-interpreter/src/evaluator/` + `tree-walk-interpreter/docs/evaluator.md`
   - `typechecker` → `tree-walk-interpreter/src/typechecker/` + `tree-walk-interpreter/docs/typechecker.md`
   - `type-inference` → `tree-walk-interpreter/src/typeinference/` + `tree-walk-interpreter/docs/typechecker.md`
   - `generics` / `traits` → `tree-walk-interpreter/src/types/`
   - `architecture` → `tree-walk-interpreter/docs/architecture.md`
   - Any label → check `tree-walk-interpreter/docs/decisions/` for ADRs governing the area

7. **Summarise** what needs to be done in 2–3 bullet points based on the acceptance criteria, so work can begin immediately.

## Notes
- Do not start implementing until the user confirms after seeing the summary.
- If the issue references an RFC, read the RFC before summarising the work.
- Commit messages for this issue must follow: `type(#<N>): description` — remind the user.
