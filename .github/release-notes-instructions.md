# Avkroken Copilot release-note instructions

## Entry guidance
- Write one concise description per qualifying pull request; do not combine separate PRs.
- Describe user- or operator-visible outcomes rather than implementation details.
- Use clear present-tense language and preserve concrete compatibility, migration, security, or data-contract effects.
- Skip pure CI, test, refactor, documentation, and dependency-maintenance work unless it has material user, operator, compatibility, or security impact.
- If the evidence is ambiguous, mark the entry uncertain rather than inventing behavior or impact.

## Safety
- Never claim that a GitHub Release implies a production deployment.
- Never expose or reconstruct secrets, tokens, credentials, private keys, or sensitive payload values.
- Do not invent fixes, behavior, deployment state, or impact not supported by the pull request or diff.
