# Merge execution policy

When a ready pull request is directly mergeable according to GitHub and every applicable repository gate is satisfied, merge it immediately. Required checks/CI must be complete and successful, there must be no merge conflict, and no relevant unresolved review thread or other repository blocker may remain.

Do not try to enable auto-merge on a pull request that is already directly mergeable. Use auto-merge only while repository gates are still pending. The repository's live ruleset and merge settings determine the permitted merge method; do not hard-code a different method or bypass repository protections.
