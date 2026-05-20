# /sprint-end

Close a sprint: create the review issue summarising outcomes, carry over incomplete issues, and update the board.

**Arguments:** `$ARGUMENTS` — sprint number, e.g. `3`

## Steps

1. **Fetch the sprint kickoff issue** to retrieve the sprint goal and planned issue list:
```bash
wsl gh issue list --repo Vladastos/Yoloscript \
  --label "sprint:kickoff" \
  --search "Sprint <N> Kickoff" \
  --json number,title,body
```

2. **Categorise planned issues** into completed and carried-over:
```bash
wsl gh issue list --repo Vladastos/Yoloscript \
  --label "status:in-progress" \
  --json number,title,state,milestone
```
Issues still open → carried over. Issues closed during the sprint → completed.

3. **Move carried-over issues back to backlog:**
```bash
wsl gh issue edit <N> --repo Vladastos/Yoloscript \
  --remove-label "status:in-progress" \
  --add-label "status:backlog"
```

4. **Create the sprint review issue:**
```bash
wsl gh issue create \
  --repo Vladastos/Yoloscript \
  --title "Sprint <N> Review" \
  --label "sprint:review" \
  --body "## Sprint Goal
<goal from kickoff issue>

## Completed
$(for each closed issue: - [x] #N Title)

## Carried Over
$(for each open issue: - [ ] #N Title)

## Epic Progress
<!-- How does this sprint advance the milestone? -->

## Spec Notes
<!-- Did any spec ambiguities surface? Link to RFC or doc-2 changes. -->

## Next Sprint Seeds
<!-- Issues or ideas for the next sprint -->"
```

5. **Close the kickoff issue** for this sprint:
```bash
wsl gh issue close <kickoff-issue-number> --repo Vladastos/Yoloscript
```

6. **Report** the review issue URL and a one-line summary of completed vs carried-over.

## Notes
- The review issue stays open for the user to fill in Epic Progress and Spec Notes.
- A sprint with 0 completed issues should still produce a review issue — record why.
- If spec ambiguities surfaced during the sprint, prompt the user to open a `/new-rfc`.
