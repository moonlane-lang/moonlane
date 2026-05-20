# /start-issue

Begin work on a GitHub issue: read its details, mark it in-progress, and surface any dependencies.

**Arguments:** `$ARGUMENTS` — issue number, e.g. `4`

## Steps

1. **Read the issue:**
```bash
wsl gh issue view <N> --repo Vladastos/Yoloscript
```
Display the title, description, acceptance criteria, labels, and milestone to the user.

2. **Check for dependencies** mentioned in the issue body (look for `#N` references or "Depends on" sections). If any dependency issues are still open, warn the user before proceeding.

3. **Mark as in-progress:**
```bash
wsl gh issue edit <N> --repo Vladastos/Yoloscript \
  --remove-label "status:backlog" \
  --add-label "status:in-progress"
```

4. **Update the GitHub Projects v2 board** status to "In Progress" via GraphQL:
   - Fetch the project item ID for this issue
   - Set the Status field to the "In Progress" option

5. **Read relevant source files** mentioned in the issue or inferable from its labels:
   - `evaluator` → `tree-walk-interpreter/src/evaluator/`
   - `typechecker` → `tree-walk-interpreter/src/typechecker/`
   - `type-inference` → `tree-walk-interpreter/src/typeinference/`
   - `generics` / `traits` → `tree-walk-interpreter/src/types/`

6. **Summarise** what needs to be done in 2–3 bullet points based on the acceptance criteria, so work can begin immediately.

## Notes
- Do not start implementing until the user confirms after seeing the summary.
- If the issue references an RFC, read the RFC before summarising the work.
- Commit messages for this issue must follow: `type(#<N>): description` — remind the user.
