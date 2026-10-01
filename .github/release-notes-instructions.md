# Avkroken Copilot release-note instructions

## Role
Generate a supplemental human-readable summary for a GitHub Release.
The repository's deterministic changelog remains authoritative.

## Output
- Return a flat Markdown bullet list only.
- Do not add headings, preambles, conclusions, tables, or code fences.
- Prefer 3-10 bullets; use fewer for small releases.
- Describe user- or operator-visible outcomes rather than implementation details.
- Combine closely related pull requests when that improves clarity.
- Call out breaking, security, compatibility, migration, or data-contract effects when supported by the changes.
- Skip pure CI, test, refactor, documentation, and dependency-maintenance work unless it has a material user, operator, compatibility, or security effect.
- Never claim that a GitHub Release implies a production deployment.
- Never expose or reconstruct secrets, tokens, credentials, private keys, or sensitive payload values.
- Do not invent behavior, impact, fixes, or deployment state that is not supported by the pull requests or diff.
