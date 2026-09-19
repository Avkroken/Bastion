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
- Named agent skills are optional accelerators, not repository dependencies. When a named skill is unavailable, follow the described workflow directly with the tools available in the current environment.
- When available, use `to-spec` for work that is materially ambiguous or broad; otherwise write down the expected behavior, scope, constraints, and acceptance checks before implementation.
- When available, use `to-tickets` when the work needs multiple independently verifiable slices; otherwise split the work into small, testable steps and verify each step before continuing.

## Implementation
- When available, use `implement` for spec- or ticket-driven work; otherwise implement the agreed scope directly.
- When available, use `tdd` when behavior can be exercised through a stable public seam; otherwise still prefer small test-first or test-backed changes where practical.
- When available, use `diagnosing-bugs` for unclear defects, regressions, flaky behavior, or performance problems; otherwise establish evidence and a root cause before changing production code.
- When available, use `research` when correctness depends on external APIs, platform behavior, standards, or current primary documentation; otherwise consult authoritative documentation directly.
- Preserve existing behavior unless the task explicitly changes it.
- Avoid unrelated cleanup in feature/fix commits.

## Design and architecture
- When available, use `codebase-design` when introducing or changing module boundaries, interfaces, dependencies, or testing seams; otherwise document the boundary and dependency implications before editing.
- When available, use `domain-modeling` when terminology or domain rules are unclear or inconsistent; otherwise make the domain assumptions explicit in the change.
- Use architecture-specific agent skills only for explicitly requested architecture work; do not silently expand a normal task into a refactor.
- Use prototyping workflows only for intentionally disposable experiments; do not merge prototype shortcuts as production architecture without review.

## Verification and review
- Run the narrowest relevant tests during development and the repository's required validation before declaring work complete.
- When available, run `code-review` against the completed diff before the final commit/PR; otherwise inspect the diff for correctness, scope, security, and missing tests.
- Treat failing CI, tests, type checks, linters, and security checks as unresolved work unless the failure is demonstrably unrelated and reported.
- When available, use `resolving-merge-conflicts` for merge/rebase conflicts; otherwise resolve conflicts by preserving the intent of both sides rather than choosing changes mechanically.

## Communication and handoff
- Keep progress concise: current state, verification performed, and any blocker or remaining risk.
- When available, use `handoff` when work must continue in another agent session; otherwise leave a concise written summary of state, evidence, changes, and remaining work.
- Use human-guidance workflows for operational steps such as credentials, secrets, provisioning, dashboards, migrations, or cutovers; never guess or fabricate access.
- Never expose secrets, tokens, credentials, private keys, or sensitive environment values in commits, logs, issues, or handoff documents.
