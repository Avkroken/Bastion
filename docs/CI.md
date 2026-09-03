# CI och merge

Repositoryts required status checks är `CI / android`, `CI / windows`, `CI / linux`, `CI / swift-linux`, `CI / apple` och `scope-policy`. De aktiva organization-level status-ruleseten använder strict latest-base-verifiering.

De fem plattformsgaterna kör den verifiering som deras statusnamn anger på varje PR. `scope-policy` verifierar filscope för namngivna `platform/*`-branches och `core/swift`; vanliga kortlivade branches har inget särskilt filscope.

Organisationens `main`-ruleset kräver den centrala OSV-workflowen från `Avkroken/.github`. På vanliga pull requests kör den `scan-pr`; i merge queue kör den `scan-merge-group`. `scan-pr / osv-scan` är inte en separat organization-level required status check.

CodeQL merge protection, review-thread resolution, squash-only och övriga gemensamma merge-regler hanteras centralt av organisationens aktiva rulesets. Repositoryt använder merge queue.

CLI- och LinuxApp-paketering för `.deb`/`.rpm` är kompletterande build/install-smoke-verifiering men inte plattforms-rulesetgates. `TestFlight` är manuellt och separat från PR-CI; signering och App Store Connect-data kommer endast från Actions secrets.

Repositoryts egen `.github/workflows/osv-scanner.yml` kör kompletterande dependency scanning. Repositoryt har ingen egen security-remediation-dispatcher, PR-watchdog, review-auto-fix eller Code Scanning-snapshotwriter.
