# /new-rfc

Create a new RFC document and register it as a GitHub issue.

**Arguments:** `$ARGUMENTS` — the RFC title (e.g. `Array literal syntax`)

## Steps

1. **Determine the next RFC number.**
   List all files in `docs/internal/rfcs/` and find the highest `rfc-NNNN` number. Increment by one. Zero-pad to four digits.

2. **Derive the slug.**
   Lowercase the title, replace spaces with hyphens, strip punctuation.
   Example: `Array literal syntax` → `rfc-0004-array-literal-syntax`

3. **Create the RFC file** at `docs/internal/rfcs/<slug>.md` using this template:

```markdown
---
id: rfc-NNNN
title: "<title>"
date: '<YYYY-MM-DD>'
status: open
---

## Summary


---

## Motivation


---

## Proposal


---

## Alternatives Considered


---

## Open Questions


---

## Timing Recommendation


---

## References

- Language spec: `docs/public/spec.md`
```

4. **Open the file** for the user to begin editing.

5. **Create the GitHub tracking issue:**

```bash
wsl gh issue create \
  --repo Vladastos/Yoloscript \
  --title "RFC-NNNN: <title>" \
  --label "type:rfc,rfc:draft" \
  --body "Tracking issue for RFC-NNNN.\n\nDoc: \`docs/internal/rfcs/<slug>.md\`"
```

6. **Commit the new file** inside the docs submodule, then update the submodule pointer in the parent repo:
```bash
cd docs && git add internal/rfcs/<slug>.md && git commit -m "docs: add RFC-NNNN <title>"
cd .. && git add docs && git commit -m "docs: update docs submodule with RFC-NNNN"
```

## Notes
- Do not start the RFC body — leave sections blank for the user to fill in.
- The GitHub issue title must match the RFC id and title exactly.
- Remind the user: the RFC must be accepted and `docs/public/spec.md` updated before implementation begins.
