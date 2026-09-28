# Issue tracker: GitHub

GitHub Issues is the canonical issue and specification tracker for this repository.

## Working convention

- Prefer the authenticated GitHub connector; otherwise use `gh` from an authenticated clone.
- Read the full issue state before acting: body, labels, comments, assignees, linked pull requests, sub-issues, and native dependencies where available.
- Keep one coherent problem or deliverable per issue, with acceptance criteria and blockers in the issue body.
- Pull requests are implementation/review artifacts, not a replacement issue tracker.
- Durable architecture or domain decisions belong in version-controlled repository documentation rather than only in issue comments.

## CLI fallback

```bash
gh issue view <number> --json number,title,body,state,labels,assignees,comments,closedByPullRequestsReferences
```

Use `gh issue list`, `gh issue create`, `gh issue comment`, and `gh issue close` for the corresponding operations. Inspect native dependency/sub-issue state through the GitHub connector or API when it affects execution order.

When a skill says to publish or fetch a ticket, use this repository's GitHub Issues tracker.
