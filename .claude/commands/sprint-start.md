# /sprint-start

Open a new sprint: create the sprint branch, the kickoff issue, assign issues to the sprint, and mark them in-progress.

**Arguments:** `$ARGUMENTS` — sprint number and goal, e.g. `3 "Implement expression evaluation"`

## Steps

1. **Parse arguments.** Extract the sprint number (integer) and the sprint goal (quoted string).

2. **Show the current backlog** for the active epic so the user can decide what goes into the sprint:
```bash
gh issue list --repo Vladastos/Gust --label "status:backlog" --json number,title,labels,milestone
```

3. **Ask the user** which issue numbers to include in this sprint before proceeding.

4. **Create and push the sprint branch:**
```bash
git checkout main && git pull
git checkout -b sprint/<N>
git push -u origin sprint/<N>
```
All sprint work must be committed to `sprint/<N>`. Nothing goes directly to `main` during the sprint.

5. **Create the sprint kickoff issue:**
```bash
gh issue create \
  --repo Vladastos/Gust \
  --title "Sprint <N> Kickoff: <goal>" \
  --label "sprint:kickoff" \
  --body "## Sprint Goal
<goal>

## Branch
\`sprint/<N>\`

## Epic Context
<!-- Which milestone does this sprint contribute to? -->

## Planned Issues
$(for each selected issue: - [ ] #N)

## Definition of Done
- All planned issues closed
- All tests pass on sprint/<N>
- PR opened against main with sprint review linked"
```

6. **Mark each planned issue as in sprint** (label only — do not move to in-progress yet):
```bash
gh issue edit <N> --repo Vladastos/Gust \
  --remove-label "status:backlog" \
  --add-label "status:in-progress"
```

7. **Update the project board** Status field to **"Todo"** for each planned issue via GraphQL. Do NOT set "In Progress" — that status is reserved for tasks actively being worked on. Sprint kickoff only moves issues from Backlog → Todo.

8. **Report** the kickoff issue URL, the sprint branch name, and the list of issues now in the sprint.

## Notes
- Sprint numbers are sequential integers (Sprint 1, Sprint 2, …).
- The sprint goal should be one sentence.
- Only issues with `status:backlog` should be moved into a sprint.
- Check CLAUDE.md for the active epic before suggesting which issues to include.
- Remind the user: all commits must go on `sprint/<N>`, not on `main`.
