# Agent workflow

## Scope
These instructions apply to all AI coding agents working in this repository.

## Git workflow
- Never make implementation commits directly on `main`.
- For new work, use a dedicated branch named `{agent}/{feature}/{YYYY-MM-DD}/{HH-mm}-{id}`.
- Keep each branch focused on one feature, fix, refactor, or maintenance task.
- Do not force-push unless the user explicitly requests it.
- Open a pull request to `main` when the work is ready for integration.

## Before implementation
- Read repository documentation, existing tests, CI workflows, and nearby code before changing behavior.
- Prefer the repository's established architecture, naming, tooling, and conventions over generic defaults.
- For non-trivial work, establish the expected behavior and testing seam before editing production code.
- Use `to-spec` for work that is materially ambiguous or broad; use `to-tickets` when the work needs multiple independently verifiable slices.

## Implementation
- Use `implement` for spec- or ticket-driven work.
- Use `tdd` when behavior can be exercised through a stable public seam; work in small red-green-refactor slices.
- Use `diagnosing-bugs` for unclear defects, regressions, flaky behavior, or performance problems instead of guessing at fixes.
- Use `research` when correctness depends on external APIs, platform behavior, standards, or current primary documentation.
- Preserve existing behavior unless the task explicitly changes it.
- Avoid unrelated cleanup in feature/fix commits.

## Design and architecture
- Apply `codebase-design` when introducing or changing module boundaries, interfaces, dependencies, or testing seams.
- Use `domain-modeling` when terminology or domain rules are unclear or inconsistent.
- Use `improve-codebase-architecture` only for an explicitly requested architecture pass; do not silently expand a normal task into a refactor.
- Use `prototype` only for intentionally disposable experiments; do not merge prototype shortcuts as production architecture without review.

## Verification and review
- Run the narrowest relevant tests during development and the repository's required validation before declaring work complete.
- Run `code-review` against the completed diff before the final commit/PR whenever practical.
- Treat failing CI, tests, type checks, linters, and security checks as unresolved work unless the failure is demonstrably unrelated and reported.
- Resolve merge/rebase conflicts with `resolving-merge-conflicts`, preserving the intent of both sides rather than choosing changes mechanically.

## Communication and handoff
- Keep progress concise: current state, verification performed, and any blocker or remaining risk.
- Use `handoff` when work must continue in another agent session.
- Use `wizard` for human-only operational steps such as credentials, secrets, provisioning, dashboards, migrations, or cutovers.
- Never expose secrets, tokens, credentials, private keys, or sensitive environment values in commits, logs, issues, or handoff documents.
