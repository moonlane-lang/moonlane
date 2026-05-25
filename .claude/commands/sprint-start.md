# /sprint-start

Open a new sprint: create the sprint branch, the kickoff issue, assign issues to the sprint, and mark them in-progress.

**Arguments:** `$ARGUMENTS` — sprint number and goal, e.g. `3 "Implement expression evaluation"`

## Steps

1. **Parse arguments.** Extract the sprint number (integer) and the sprint goal (quoted string).

2. **Determine the active milestone.** Read `CLAUDE.md` for the current development focus milestone (e.g. `v0.3`). This milestone must be applied to the kickoff issue and all planned issues. If the milestone is ambiguous, ask the user before continuing.

3. **Show the current backlog** for the active epic so the user can decide what goes into the sprint:
```bash
gh issue list --repo moonlane-lang/moonlane --label "status:backlog" --json number,title,labels,milestone
```

4. **Ask the user** which issue numbers to include in this sprint before proceeding.

5. **Create and push the sprint branch:**
```bash
git checkout main && git pull
git checkout -b sprint/<N>
git push -u origin sprint/<N>
```
All sprint work must be committed to `sprint/<N>`. Nothing goes directly to `main` during the sprint.

6. **Create the sprint kickoff issue** with the active milestone:
```bash
gh issue create \
  --repo moonlane-lang/moonlane \
  --title "Sprint <N> Kickoff: <goal>" \
  --label "sprint:kickoff" \
  --milestone "<milestone>" \
  --body "## Sprint Goal
<goal>

## Branch
\`sprint/<N>\`

## Milestone
<milestone>

## Planned Issues
$(for each selected issue: - [ ] #N)

## Definition of Done
- All planned issues closed
- All tests pass on sprint/<N>
- PR opened against main with sprint review linked"
```

7. **Ensure all planned issues carry the correct milestone:**
```bash
gh issue edit <N> --repo moonlane-lang/moonlane \
  --milestone "<milestone>" \
  --remove-label "status:backlog" \
  --add-label "status:in-progress"
```
Run this for every planned issue. Issues must not enter a sprint without a milestone.

8. **Update the project board** Status field to **"Todo"** for each planned issue via GraphQL. Do NOT set "In Progress" — that status is reserved for tasks actively being worked on. Sprint kickoff only moves issues from Backlog → Todo.

9. **Report** the kickoff issue URL, the sprint branch name, the milestone, and the list of issues now in the sprint.

## Notes
- Sprint numbers are sequential integers (Sprint 1, Sprint 2, …).
- The sprint goal should be one sentence.
- Only issues with `status:backlog` should be moved into a sprint.
- Check CLAUDE.md for the active epic and milestone before proceeding.
- All issues created or touched during the sprint must carry the active milestone — kickoff, review, planned issues, and any new issues opened mid-sprint.
- Remind the user: all commits must go on `sprint/<N>`, not on `main`.
