# /sprint-end

Close a sprint: run tests, carry over incomplete issues, open the sprint review issue and PR, then hand off to the user.

**Arguments:** `$ARGUMENTS` — sprint number, e.g. `3`

## Steps

1. **Fetch the sprint kickoff issue** to retrieve the sprint goal, planned issues, and kickoff issue number:
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

4. **Ensure all tests pass on the sprint branch:**
```bash
cd tree-walk-interpreter && cargo test
```
If any tests fail, do not proceed — fix them first.

5. **Create the sprint review issue:**
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
$(for each open issue: - [ ] #N Title — reason if known)

## Epic Progress
<!-- How does this sprint advance the milestone? -->

## Spec Notes
<!-- Did any spec ambiguities surface? Link to any RFC or docs/public/spec/ changes. -->

## Next Sprint Seeds
<!-- Issues or ideas for the next sprint -->"
```
Note the issue number returned — it is needed for the PR body.

6. **Open a pull request** from `sprint/<N>` → `main`:
```bash
gh pr create \
  --repo Vladastos/Yoloscript \
  --base main \
  --head sprint/<N> \
  --title "Sprint <N> — <theme>" \
  --body "$(cat <<'EOF'
Sprint review: #<review-issue-number>

Closes #<review-issue-number>
Closes #<kickoff-issue-number>
EOF
)"
```
Both `Closes` lines are required. On merge, GitHub automatically closes the sprint review issue and the kickoff issue.

7. **Leave a note for the user:**

> **Sprint <N> is ready for review.**
>
> - Fill in the review issue: #<review-issue-number>
> - Review the PR diff and approve it on GitHub
> - **Merge the PR** — this automatically closes the review issue (#<review-issue-number>) and the kickoff issue (#<kickoff-issue-number>)
> - After merging, delete the `sprint/<N>` branch on GitHub

## Notes
- A sprint with 0 completed issues should still produce a review issue — record why.
- Do not close the kickoff issue manually — the PR merge closes it via `Closes #N`.
- Do not close the review issue manually — the PR merge closes it via `Closes #N`.
- If spec ambiguities surfaced during the sprint, prompt the user to open a `/new-rfc`.
- The sprint branch must not be deleted until after the PR is merged.
