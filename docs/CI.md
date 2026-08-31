# CI och branchflöde

## Branchmodell

`main` är den enda långlivade arbetsgrenen. Varje ändring görs på en kortlivad branch och går via PR till `main`. Auto-merge får aktiveras först när det aktiva rulesetet motsvarar mergekontraktet nedan. **Squash merge är den enda tillåtna merge-metoden.** Head-branchen raderas automatiskt efter merge.

CI körs på PR mot `main` och, där efter-merge-verifiering eller publicering behövs, på push till `main`. Kortlivade arbetsbrancher behöver ingen separat push-CI när samma commit redan verifieras av PR-eventet.

## Mergekontrakt

När den här ändringens ruleset är importerat får `main` endast uppdateras när senaste PR-HEAD har verifierats av:

- `CI / android`
- `CI / windows`
- `CI / linux`
- `CI / swift-linux`
- `CI / apple`
- `scope-policy`
- `scan-pr / osv-scan`
- CodeRabbits kanoniska review-progress för exakt aktuell HEAD
- GitHubs Code Scanning merge protection för CodeQL

De fem `CI / …`-jobben är stabila aggregate-gates. De kör alltid. Ett internt plattformsjobb får vara `skipped` när impact-routern uttryckligen bedömer plattformen som opåverkad; aggregate-jobbet blir då success. Om impact-jobbet eller en relevant build/test misslyckas måste motsvarande aggregate-gate misslyckas.

CodeRabbit ska använda `review_progress: true`, `fail_commit_status: true`, incremental review efter varje push och `auto_pause_after_reviewed_commits: 0`. Merge får endast ske när CodeRabbit har avslutat review av exakt aktuell HEAD. Legacy commit-status får inte användas som mergebevis eftersom rate limiting kan rapporteras som `success`. Queued, in-progress, rate-limited, failure, saknad review eller review av en äldre HEAD blockerar. Copilot Code Review är rådgivande och ska köras om efter push, men är inte en mergegate eftersom tillgänglighet och quota inte är deterministiska.

CodeQL ska inte hanteras som en vanlig statuscheck-lista. Rulesetets Code Scanning-regel ska blockera medan analys saknas/pågår och vid fynd över den valda tröskeln.

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
