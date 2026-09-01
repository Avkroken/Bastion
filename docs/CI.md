# CI och branchflöde

## Branchmodell och live merge-policy

`main` är den enda långlivade arbetsgrenen. Ändringar går via ready pull request från en kortlivad branch och squash är enda tillåtna merge-metod.

Organisationens aktiva rulesets är verkställande sanning. Vid senaste live-verifieringen krävs på aktuell PR-HEAD:

- `CI / android`
- `CI / windows`
- `CI / linux`
- `CI / swift-linux`
- `CI / apple`
- `scope-policy`
- `scan-pr / osv-scan`

Required status checks är strict mot aktuell `main`. Default-branch-policyn kräver dessutom 1 approval, avfärdar stale approvals efter push, kräver last-push approval från någon annan än senaste pushern, kräver resolved review threads, blockerar deletion/force push och har inga bypass actors.

CodeQL verkställs av Code Scanning merge protection med `medium_or_higher` för security alerts och `errors_and_warnings` för övriga alerts. Copilot Code Review och CodeRabbit är rådgivande; faktiska findings ska utvärderas men tjänsternas tillgänglighet är inte i sig en required status check.

Org-rulesetet `main` refererar fortfarande till Regelverkets `.github/workflows/osv-scanner.yml` som central required workflow. Det är organisationsnivå och måste ändras separat när den centrala OSV-kopplingen tas bort.

## Direkta plattformsgates

Bastion använder inte längre `.github/scripts/ci-impact.sh`, dess testscript eller `ci-impact-test.yml`. De fem required plattformsworkflows kör i stället den verifiering som deras statusnamn påstår på varje PR:

- `CI / android` — Gradle build/test för Android.
- `CI / windows` — .NET core-tester och WinUI-build.
- `CI / linux` — Rust/GTK4 workspace build/test samt deklarerad MSRV-build.
- `CI / swift-linux` — Swift package build/test i Linux-container.
- `CI / apple` — XcodeGen, iOS/macOS/tvOS build, SwiftPM på macOS och iOS-screenshot smoke path.

Det gör required-gates enklare att resonera om och eliminerar en separat routingmotor som kunde ge falska negativa skip-beslut. Kostnaden är att fler plattformsjobb körs på dokumentations- och workflowändringar; det är ett medvetet fail-closed-val.

## Scope policy

`.github/workflows/scope-policy.yml` producerar `scope-policy`. Vanliga kortlivade branches har inget särskilt filscope. Namngivna `platform/*`-branches och `core/swift` har avgränsat filscope; workflowen verifierar base/head-SHA, kräver en giltig icke-tom diff och failar om en ändrad fil ligger utanför tillåtet område.

## Paketering och release

CLI `.deb`/`.rpm` samt LinuxApp `.deb`/`.rpm` är kompletterande build/install-smoke-verifiering. De är inte de fem plattforms-rulesetgates men körs på PR för att upptäcka paketeringsregressioner.

`TestFlight` är manuellt och separat från PR-CI. Signering och App Store Connect-data kommer endast från Actions secrets och ska aldrig committas.

## Security

`.github/workflows/osv-scanner.yml` är repositoryts egen dependency-vulnerability scan. PR-jobbet producerar `scan-pr / osv-scan`; `main`/schedule/manual används för kompletterande rapportering.

Repositoryt har ingen egen security-remediation-dispatcher, PR-watchdog, review-auto-fix eller Code Scanning-snapshotwriter. GitHub Actions ska inte skapa/uppdatera branches eller PR:er eller arma auto-merge.
