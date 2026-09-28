# Domain documentation

This repository uses a **single-context** domain-documentation convention.

## Read before domain-sensitive work

1. `docs/project-context.md` for current technical state.
2. `docs/architecture.md` for component and trust-boundary decisions.
3. `docs/operations.md` for verified operational constraints.
4. Relevant ADRs under `docs/adr/`, when present.
5. Root `CONTEXT.md`, when an explicit domain glossary/context has been introduced.

Absence of `CONTEXT.md` or `docs/adr/` is not an error. Do not create placeholder files merely to satisfy the convention; create or update them when domain terminology or architectural decisions actually need durable representation.

Use established repository vocabulary consistently. If a proposal conflicts with an existing recorded decision, surface that conflict explicitly rather than silently overriding it.
