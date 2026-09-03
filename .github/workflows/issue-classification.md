---
description: >-
  Metadata-only AI triage for new and reopened issues. Applies exactly one
  temporary classification label; deterministic routing converts it to the
  canonical difficulty and security labels.
on:
  workflow_call:
  roles: all
engine: copilot
permissions:
  contents: read
  issues: read
safe-outputs:
  add-labels:
    allowed:
      - "classification:low:critical"
      - "classification:low:high"
      - "classification:low:medium"
      - "classification:low:low"
      - "classification:low:none"
      - "classification:medium:critical"
      - "classification:medium:high"
      - "classification:medium:medium"
      - "classification:medium:low"
      - "classification:medium:none"
      - "classification:high:critical"
      - "classification:high:high"
      - "classification:high:medium"
      - "classification:high:low"
      - "classification:high:none"
    max: 1
    target: triggering
    create-if-missing: true
    issues: true
    pull-requests: false
  missing-tool:
    create-issue: false
  missing-data:
    create-issue: false
  report-incomplete:
    create-issue: false
  noop:
    report-as-issue: false
  report-failure-as-issue: false
---

# Avkroken metadata-only issue triage

Analyze only the triggering issue title and body, plus read-only repository context when needed to understand scope.

Apply exactly one temporary classification label. Its format is:

`classification:<difficulty>:<security>`

Choose exactly one difficulty value:
- `low`: localized, well-scoped change with low uncertainty and little cross-component coordination.
- `medium`: multiple files/components, meaningful investigation, integration work, or moderate uncertainty.
- `high`: architectural or cross-system work, substantial uncertainty, migration/concurrency/security complexity, or a large coordinated change.

Choose exactly one security value:
- `critical`: credible immediate risk of severe compromise, broad unauthorized access, secret exposure with major blast radius, or similarly urgent security impact.
- `high`: credible significant security impact requiring prompt remediation but not meeting critical criteria.
- `medium`: bounded or conditional security impact with meaningful risk.
- `low`: minor defense-in-depth/security hardening issue with limited practical impact.
- `none`: no credible security impact is described or implied by the issue.

Security classification is about security impact, not general product urgency. Do not use a security value to represent outage severity, feature importance, or business priority. Do not invent a security impact that is not supported by the issue or repository context; use `none` when there is no credible security dimension.

Do not add any other labels. Do not comment, assign users or agents, create or update branches or pull requests, start an agent session, edit issue text, close issues, perform review, merge anything, or modify repository content. Do not propose or perform remediation. Do not create issues or other fallback records when tools, data, inference, or workflow execution fails; fail the workflow instead. The deterministic metadata workflow converts the single temporary classification label into exactly one canonical `difficulty:*` and one canonical `security:*`, removes the temporary label, and then handles owner assignment, `agent:*`, `priority:*`, and triage state.
