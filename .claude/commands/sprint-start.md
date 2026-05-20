# /sprint-start

Open a new sprint: create the kickoff issue, assign issues to the sprint iteration, and mark them in-progress.

**Arguments:** `$ARGUMENTS` — sprint number and goal, e.g. `3 "Implement expression evaluation"`

## Steps

1. **Parse arguments.** Extract the sprint number (integer) and the sprint goal (quoted string).

2. **Show the current backlog** for the active epic so the user can decide what goes into the sprint:
```bash
wsl gh issue list --repo Vladastos/Yoloscript --label "status:backlog" --json number,title,labels,milestone
```

3. **Ask the user** which issue numbers to include in this sprint before proceeding.

4. **Create the sprint kickoff issue:**
```bash
wsl gh issue create \
  --repo Vladastos/Yoloscript \
  --title "Sprint <N> Kickoff: <goal>" \
  --label "sprint:kickoff" \
  --body "## Sprint Goal
<goal>

## Epic Context
<!-- Which milestone does this sprint contribute to? -->

## Planned Issues
$(for each selected issue: - [ ] #N)

## Definition of Done
<!-- What does sprint complete look like? -->"
```

5. **Mark each planned issue as in-progress:**
```bash
wsl gh issue edit <N> --repo Vladastos/Yoloscript \
  --remove-label "status:backlog" \
  --add-label "status:in-progress"
```

6. **Assign issues to the Sprint iteration** in GitHub Projects v2 using the GraphQL API. For each issue:
   - Get the project item ID for the issue
   - Set the Sprint field to the current iteration

7. **Report** the kickoff issue URL and the list of issues now in the sprint.

## Notes
- Sprint numbers are sequential integers (Sprint 1, Sprint 2, …).
- The sprint goal should be one sentence.
- Only issues with `status:backlog` should be moved into a sprint.
- Check CLAUDE.md for the active epic before suggesting which issues to include.
