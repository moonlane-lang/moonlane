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
status: draft
target:
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

4. **Fill in the RFC body.** If the user's request or the current conversation contains enough context to write the RFC sections (motivation, proposal, alternatives, open questions), fill them in now. Leave sections blank only when there is genuinely insufficient information.

5. **Create the GitHub tracking issue:**

```bash
gh issue create \
  --repo gust-lang/gust \
  --title "RFC-NNNN: <title>" \
  --label "type:rfc,rfc:draft" \
  --body "Tracking issue for RFC-NNNN.\n\nDoc: \`docs/internal/rfcs/<slug>.md\`"
```

6. **Commit the new file** directly in the repo:
```bash
git add docs/internal/rfcs/<slug>.md && git commit -m "docs: add RFC-NNNN <title>"
```

## Notes
- The GitHub issue title must match the RFC id and title exactly.
- The `target:` field is left blank until the RFC is accepted and assigned to a version.
- Remind the user: the RFC must be accepted (status → `accepted`, `target:` set) and the relevant `docs/public/spec/` file updated before implementation begins.
- When an RFC is incorporated into a release, mark it `incorporated` and commit the spec change.
