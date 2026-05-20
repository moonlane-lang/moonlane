# /ship-issue

Complete an issue: verify acceptance criteria, commit with the correct format, and close on GitHub.

**Arguments:** `$ARGUMENTS` — issue number, e.g. `4`

## Steps

1. **Read the issue** to retrieve title, acceptance criteria, and milestone:
```bash
wsl gh issue view <N> --repo Vladastos/Yoloscript
```

2. **Verify acceptance criteria.** Go through each checkbox in the issue body and confirm it is met. If any criterion is unmet, stop and tell the user what remains.

3. **Run the test suite** from `tree-walk-interpreter/`:
```bash
cd tree-walk-interpreter && cargo test
```
Do not proceed if tests fail.

4. **Check git status** for staged and unstaged changes:
```bash
git status
git diff
```

5. **Compose the closing commit message** following the project convention:
```
type(#<N>): <short description>

- <bullet: what was done>
- <bullet: what was done>
- <bullet: what was done>

Closes #<N>
Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>
```
`type` is one of: `feat`, `fix`, `refactor`, `test`, `docs`.
`Closes #<N>` in the body auto-closes the issue on push.

6. **Commit** with the composed message. Stage only files relevant to the issue — do not use `git add -A`.

7. **Verify the issue closes automatically on push**, or close it manually if not pushing immediately:
```bash
wsl gh issue close <N> --repo Vladastos/Yoloscript
```

8. **Update the GitHub Projects v2 board** status to "Done" via GraphQL.

9. **Check** whether this was the last open issue in the current sprint. If so, prompt the user to run `/sprint-end`.

## Notes
- The closing commit body must be a bullet list of what was done — not a paraphrase of the issue title.
- If the issue required a spec update, verify `docs/public/spec.md` was modified and committed in the docs submodule first.
- If the issue resolved an RFC open question, update the RFC status field to `accepted` and note the resolution.
