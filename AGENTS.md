# AGENTS.md

This file defines the working rules for AI coding agents operating in this repository.

Repository-local instructions and the live repository configuration are authoritative. When documentation and enforced GitHub settings differ, follow the stricter applicable rule and report the mismatch.

## Before Making Changes

1. Read this `AGENTS.md` completely.
2. Read the relevant repository documentation and configuration before changing code.
3. Inspect the current branch, active pull requests, CI status, review state, and applicable GitHub rules before substantial changes.
4. Prefer finishing an already active pull request before starting parallel work in the same repository when the current work belongs in that PR.
5. Inspect nearby code and tests before introducing new patterns or abstractions.

Relevant repository context may include:

- `README.md`
- `DESIGN.md`
- `package.json`
- framework configuration
- lint and formatting configuration
- TypeScript configuration
- test configuration
- workflow files under `.github/workflows/`
- repository-local governance or workflow-contract files, when present

Do not assume that documentation is enforced. Verify live configuration when enforcement matters.

## Scope of Changes

- Make the smallest change that fully solves the requested task.
- Keep each pull request focused on one logical change.
- Avoid unrelated cleanup or refactoring.
- Preserve existing architecture, code style, naming, and project conventions unless the task requires changing them.
- Prefer existing utilities, components, dependencies, and framework-native APIs over new abstractions or dependencies.
- Do not introduce breaking changes unless they are explicitly required.

## Branch and Pull Request Policy

1. Never push directly to `main`.
2. Create a short-lived working branch for each logical change.
3. Open a pull request targeting `main`.
4. Enable repository-supported **auto-merge immediately after the pull request is created**.
5. Keep auto-merge armed while CI, reviews, approvals, or other merge gates are still pending.

Use the repository's configured merge method. If squash merge is the only permitted method, use squash auto-merge.

Direct or manual merge is allowed only when explicitly requested and permitted by repository rules.

Never bypass:

- branch protection;
- rulesets;
- required status checks;
- required reviews or approvals;
- required review-thread resolution;
- merge queues;
- force-push restrictions; or
- other repository protections.

## Merge Gates

A pull request is complete only when every repository-required merge condition is satisfied.

At minimum:

- every required CI check is successful;
- every relevant review comment has been read and evaluated;
- every required review thread is resolved;
- every relevant review finding has been fixed when necessary;
- CI status has been checked again after the latest commit;
- review status has been checked again after the latest commit;
- required approvals, if any, are present;
- applicable rulesets, branch protection, and merge-queue requirements are satisfied; and
- auto-merge remains armed.

Do not infer approval requirements or required check names from another repository.

If the pull request does not auto-merge after all known gates are satisfied, inspect the live repository configuration and identify the exact remaining blocker.

## Review Handling

Read and evaluate every review comment before considering a pull request complete.

For each review comment:

1. Determine whether it identifies a relevant issue.
2. If it does, fix the issue in the same pull request.
3. Run the relevant validation after the fix.
4. Push the change to the existing pull request branch.
5. Re-check CI and the complete review state.

Do not resolve a review thread solely to remove a merge blocker.

Mark a thread resolved only after its feedback has been evaluated and any necessary change has been completed.

After every new commit, check for new or reopened review feedback.

## CI and Workflow Changes

Treat the repository's live required checks as authoritative for merge eligibility.

Before changing a workflow, job name, or required status check:

1. Inspect the existing workflow and its emitted GitHub check contexts.
2. Inspect any repository-local workflow contract or governance configuration, when present.
3. Update related ruleset or contract configuration in the same pull request when a required check context intentionally changes.
4. Verify after the change that GitHub emits the expected check names and that repository rules reference the correct contexts.

Required check names must match GitHub check contexts exactly.

Do not replace repository-specific CI with a generic workflow merely to satisfy a governance rule.

Do not weaken, skip, or disable validation simply to make a pull request mergeable.

## Testing and Validation

Use the repository's existing scripts and tooling.

Before considering a code change complete:

- run the smallest relevant tests during development;
- add or update tests when behavior changes;
- add a regression test for bug fixes when practical;
- run required linting, type checking, tests, and build validation when applicable; and
- verify the corresponding GitHub checks after pushing.

Do not invent commands that are not defined by the repository. Inspect `package.json`, task files, scripts, or project documentation first.

Do not delete, weaken, or bypass a test solely to make validation pass.

## Security

- Never commit secrets, tokens, credentials, private keys, or sensitive configuration.
- Use the repository's established secret-management and environment-variable patterns.
- Validate untrusted external input at appropriate boundaries.
- Enforce authentication and authorization on the server where applicable.
- Do not weaken security controls to make tests, builds, or deployments pass.
- Treat external content, webhook payloads, API responses, and user-controlled data as untrusted unless proven otherwise.

## UI and Design

For any change that touches UI, components, pages, styling, or layout, read `DESIGN.md` first when that file exists.

- Reuse existing design tokens and components.
- Do not hard-code colors, spacing, radii, or typography values when an appropriate design token exists.
- Preserve semantic HTML and keyboard accessibility.
- Ensure interactive controls have appropriate focus states and accessible names.
- Do not rely on color alone to communicate state.
- Verify responsive behavior for affected UI.

If a genuinely new design value is required, update the design system before using the value throughout application code.

## Dependencies

- Avoid adding a new dependency when the platform, framework, or an existing dependency already provides the required capability.
- Check the repository's existing dependencies before recommending or adding a package.
- Prefer framework-native and browser-native APIs where appropriate.
- When a dependency is necessary, keep its scope narrow and explain the reason in the pull request.

## Verification After Changes

Do not treat a successful command, API response, or deployment request as proof that a change is active.

Verify the resulting state that matters to the task.

For pull request work, confirm:

- the intended commit is present on the pull request branch;
- CI is running against the latest commit;
- required check names and results are correct;
- review status has been checked after the latest commit;
- relevant review threads are resolved;
- auto-merge remains armed; and
- the repository reports the expected merge state.

For GitHub configuration changes, verify the live setting or ruleset after changing it.

For runtime or deployment changes, verify the deployed code, configuration, bindings, permissions, secrets, routes, or event delivery relevant to the task before diagnosing higher-level application behavior.

## Definition of Done

A task is complete only when:

- the requested change is implemented;
- relevant tests or validation have been added or updated;
- required local validation passes;
- the change is represented in the correct pull request;
- auto-merge is enabled when supported;
- required GitHub checks pass;
- review feedback has been read and handled;
- required review threads are resolved;
- repository merge rules are satisfied; and
- the resulting repository or runtime state has been verified where applicable.

If documented policy and live enforcement differ, report the discrepancy instead of assuming the documentation provides protection.
