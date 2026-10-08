# Automated security remediation

The repository-local `.github/workflows/security-alert-issues.yml` runs every
hour and can be dispatched manually. It reconciles open Code Scanning and
Dependabot alerts into sanitized GitHub Issues and requests Copilot cloud agent
work on eligible pending issues. It recognizes pre-existing
`skvallerbyttan-alert` markers to avoid duplicate tracking issues.

For public repositories, **secret-scanning findings remain private**:
publishing them as public Issues would violate the security disclosure policy.
They need a confidential tracker. The script does not copy secrets, raw
findings, or exploit details into public issue bodies.

The owner `Avkroken` is requested as issue assignee. Copilot is a special
GitHub coding-agent assignment and may open a draft remediation PR once the
agent has an actual code change; successful assignment does not guarantee
a PR. Codex and Claude require separate integrations to receive work; CodeRabbit
reviews pull requests when enabled and is not an ordinary issue assignee.
No fake assignments are reported. Existing CI, CodeQL advanced setup, rulesets,
and manual/auto-merge policies are unchanged.

## Permissions and failure reporting

The workflow uses the built-in `GITHUB_TOKEN` with contents read, issues write,
pull requests read and security events read. **That token may not grant the
separate Dependabot alert read permission.** An unsupported or forbidden alert
read reports an error rather than falsely claiming all alerts have been
handled. Do not invent a new token or secret; verify an existing credential's
name and minimal scopes before integrating it. Skvallerbyttan stays read-only.

A run can create/reopen up to 100 tracking issues and request up to three
Copilot assignments. Later scheduled runs resume remaining work.
Issues generated with `GITHUB_TOKEN` do not themselves trigger a new
`issues` workflow, so agent requests are done in the same run.
No empty placeholder PRs are created or auto-merged.

Test with `node --test .github/scripts/security-reconcile.test.mjs`.
