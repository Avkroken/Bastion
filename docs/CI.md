# CI och branchflöde

## Branchmodell

`main` är den enda långlivade arbetsgrenen. Varje ändring görs på en kortlivad branch och går via PR till `main`. Auto-merge får aktiveras först när det aktiva rulesetet motsvarar mergekontraktet nedan och aktuell PR-HEAD uppfyller samtliga obligatoriska gates. **Squash merge är den enda tillåtna merge-metoden.** Head-branchen raderas automatiskt efter merge.

CI körs på PR mot `main` och, där efter-merge-verifiering eller publicering behövs, på push till `main`. Kortlivade arbetsbrancher behöver ingen separat push-CI när samma commit redan verifieras av PR-eventet.

## Mergekontrakt

Det aktiva repository-rulesetet `main-protection` gäller default branch. Det har inga bypass-aktörer, blockerar deletion och force push samt kräver PR med squash merge, 0 generella approvals, ingen last-push-approval och lösta review-trådar.

Följande status checks är obligatoriska på senaste PR-HEAD:

- `CI / android`
- `CI / windows`
- `CI / linux`
- `CI / swift-linux`
- `CI / apple`
- `scope-policy`
- `scan-pr / osv-scan`

Required-status-policyn är strict, vilket innebär att PR:n måste vara verifierad mot senaste `main` innan merge.

De fem `CI / …`-jobben är stabila aggregate-gates. De kör alltid. Ett internt plattformsjobb får vara `skipped` när impact-routern uttryckligen bedömer plattformen som opåverkad; aggregate-jobbet blir då success. Om impact-jobbet eller en relevant build/test misslyckas måste motsvarande aggregate-gate misslyckas.

CodeQL hanteras av rulesetets Code Scanning merge protection, inte som en vanlig required status check. Tröskeln är `errors_and_warnings` för vanliga alerts och `medium_or_higher` för security alerts.

Copilot Code Review är rådgivande, har `review_on_push: true` och är inte en mergegate eftersom tillgänglighet och quota inte är deterministiska.

CodeRabbit är best-effort och är inte en required status check. Konfigurationen använder `review_progress: true`, `fail_commit_status: true`, incremental review efter varje push och `auto_pause_after_reviewed_commits: 0`. Saknad, väntande, rate-limitad eller otillgänglig CodeRabbit-review blockerar inte ensam merge. Faktiska findings ska däremot utvärderas och relevanta review-trådar måste lösas innan merge.

## Impact-routing

`.github/scripts/ci-impact.sh` klassificerar diffen till Apple, root-Swift/SSHCore, Android, Windows, LinuxApp och CLI-paketering.

- Apple UI/projekt => Apple-jobb.
- `Sources/SSHCore/**` => Apple + root-Swift + CLI-paketering.
- `Sources/bastion-cli/**` => root-Swift + CLI-paketering.
- `Tests/SSHCoreTests/**` => root-Swift.
- `Android/**`, `WindowsApp/**`, `LinuxApp/**` => respektive plattform.
- Okänd kod/config eller ändring i impact-motorn => full matris.

Required checks får inte filtreras bort på workflow-nivå med `paths:` om det kan lämna dem i `Expected/Pending`. Required workflows använder därför billiga impact-jobb och job-level `if:` för dyrt arbete. Routingtabellen testas av `.github/scripts/test-ci-impact.sh` och `.github/workflows/ci-impact-test.yml`.

Målet är att verifiera relevant kod utan att köra hela plattformsmatrisen i onödan. Vid osäkerhet körs mer CI, inte mindre.

Den tidigare workflowen `.github/workflows/required-ci.yml` är borttagen. Dess `CI / required` syntaxkontrollerade endast impact-scriptet och var därför inte ett korrekt aggregate-bevis för plattforms-CI.
