# AGENTS.md

Den här filen är repositoryts auktoritativa arbetsinstruktion för AI-agenter. Live GitHub-konfiguration är verkställande sanning: om dokumentation och faktisk enforcement skiljer sig ska live-regeln följas och mismatchen rapporteras konkret.

Läs även eventuell närmare `AGENTS.md` för filer som berörs samt `SKILLS.md`, som styr svarsformatet i detta repository.

## Arbetsprincip

Leverera fungerande, verifierade och avgränsade ändringar. Bevara befintlig arkitektur och repository-specifika konventioner om det inte finns ett konkret skäl att ändra dem. CI och review ska verifiera arbetet, inte vara första debuggern för fel som rimligen kan upptäckas före PR.

Innan implementation: läs relevant kod, tester, konfiguration och dokumentation; identifiera repositoryts faktiska build-, test-, lint- och CI-kommandon; gör minsta kompletta ändring; kontrollera att inga secrets, credentials, debugrester eller oavsiktliga filer läggs till.

## Brancher och pull requests

- Pusha aldrig direkt till `main`.
- Skapa en kortlivad arbetsgren för varje logisk ändring och öppna en ready PR mot `main` efter pre-PR-granskning.
- Aktivera auto-merge först när live-rulesetet faktiskt motsvarar mergekontraktet nedan. Om ruleset-migration pågår ska PR:n lämnas öppen tills den nya policyn är aktiv och verifierad.
- Direkt merge får endast användas om repositoryägaren uttryckligen begär det.
- Squash är enda tillåtna merge-metod.
- Repositoryt använder inte merge queue.
- Kringgå aldrig branch protection, rulesets, required checks, reviews eller review-thread resolution.

De befintliga `platform/*`- och `core/swift`-brancherna har specialscope i `.github/workflows/scope-policy.yml`, men de utgör inte en obligatorisk branchpool för nytt arbete. `work/*` och `docs/content` ska inte återställas eller synkas automatiskt av repositoryautomation.

## Merge-gates och review

För `main` ska följande vara verifierat på senaste PR-HEAD när det nya rulesetet är aktivt:

- `CI / android`
- `CI / windows`
- `CI / linux`
- `CI / swift-linux`
- `CI / apple`
- `scope-policy`
- `scan-pr / osv-scan`
- CodeRabbits kanoniska review-progress för exakt aktuell HEAD
- Code Scanning merge protection för CodeQL
- lösta review-trådar

De fem `CI / …`-jobben är stabila aggregate-gates. Interna plattformsjobb får vara `skipped` endast när impact-routern uttryckligen bedömer plattformen som opåverkad; aggregate-jobbet ska då verifiera detta och bli success. Om impact-jobbet eller relevant build/test misslyckas ska aggregate-gaten misslyckas.

CodeRabbit är merge-reviewgaten. `.coderabbit.yaml` ska använda `review_progress: true`, `fail_commit_status: true`, incremental review på varje push och ingen automatisk paus. Legacy-läget med `review_progress: false` och `commit_status: true` får inte användas som mergebevis eftersom `success` kan publiceras när reviewn rate-limitats. Queued, in-progress, rate-limited, failure, saknad review eller review av en äldre HEAD blockerar merge. Copilot Code Review är rådgivande, ska köras om efter push, men är inte hard gate eftersom quota/tillgänglighet inte är deterministisk.

Required checks och olösta review-trådar är merge-blockerare. Alla review-kommentarer ska läsas och utvärderas; relevanta findings åtgärdas i samma PR. En tråd markeras resolved först när eventuell nödvändig fix är pushad och verifierad.

Efter varje ny commit eller push ska aktuell HEAD, required checks, mergeability, mergekonflikter, review-sammanfattningar och öppna/återöppnade review-trådar kontrolleras igen.

## CI-design

`.github/scripts/ci-impact.sh` äger selektiv routing. Required workflows använder billiga impact-jobb och job-level `if:` för dyrt arbete. Vid okänd påverkan ska mer CI köras, inte mindre.

Den tidigare `.github/workflows/required-ci.yml` är avsiktligt borttagen. Dess `CI / required` syntaxkontrollerade endast impact-scriptet och var inte ett verkligt aggregate-bevis för plattforms-CI.

`.github/workflows/osv-scanner.yml`, paketerings-, release-, security- och deploy-workflows som inte uttryckligen listas som merge-gates ovan är kompletterande verifiering. CodeQL ska säkras genom rulesetets Code Scanning merge protection, inte genom en skör lista av dynamiska analysjobb.

## Pre-PR quality gate

Granska hela diffen mot base branch före ready PR. Kontrollera korrekthet, säkerhet, felhantering, kompatibilitet och relevanta edge cases. Kör tillämpliga tester, lint, typecheck och build. Lägg till eller uppdatera tester när beteende ändras och detta är praktiskt testbart. Om full lokal validering inte är möjlig ska begränsningen beskrivas konkret i PR:n; hitta inte på gröna resultat.

Prioritera funktionell och teknisk review-signal: korrekthet, säkerhet, tillförlitlighet, kompatibilitet, tester och underhållbarhet. Redaktionell puts i prosa är inte ett finding om den inte ändrar teknisk betydelse eller maskinläsbar semantik.

## Reviewnivå och eskalering

Använd lägsta reviewnivå som ger tillräcklig säkerhet. Rutinmässiga lokala ändringar kan använda Copilot Lite; icke-trivial logik eller flera komponenter använder Balanced. Auth/access control, credentials, persistent data/schema, concurrency/retries, cross-service state, integrationskontrakt, releaseflöden eller privilegierad infrastruktur kräver minst Balanced och vid behov ett oberoende Codex-pass. Exceptionell risk för auth bypass, secret exposure eller dataförlust kräver ytterligare oberoende review. Bygg inte nya AI-router-workflows enbart för eskalering och lägg inte till externa AI-provider-credentials utan uttryckligt godkännande.

## Säkerhet

Committa eller exponera aldrig secrets, tokens, privata nycklar eller andra credentials. Använd repositoryts etablerade secret-management- och environment-variable-mönster. Validera opålitlig extern input vid rätt boundary och upprätthåll auth/authz server-side där det är relevant. Försvaga inte säkerhetskontroller för att göra tester eller builds gröna.

## UI och design

För ändringar som berör UI, components, pages, styling eller layout ska `DESIGN.md` läsas först när filen finns. Återanvänd design tokens/components, bevara semantisk HTML och keyboard accessibility, säkerställ focus states/accessibility names och använd inte enbart färg för state. Verifiera relevant responsive behavior.

## Dependencies

Undvik nya dependencies när plattformen, ramverket eller en befintlig dependency redan löser behovet. Håll nödvändiga nya dependencies snävt avgränsade och motivera dem i PR:n.

## Verifiering efter ändringar

Ett lyckat API-svar, workflow-anrop eller deployment-request är inte bevis på att ändringen är aktiv. När GitHub-inställningar, permissions, deployments, routes, bindings eller annan live/runtime-konfiguration ändras ska resulterande state verifieras efteråt.

## Definition of done

En PR-baserad uppgift är klar först när implementationen är färdig och avgränsad, relevanta tester/checks har körts eller en konkret begränsning dokumenterats, slutdiffen självgranskats, all review-feedback utvärderats, legitima findings åtgärdats, aktuell HEAD och required CI verifierats efter senaste commit, alla relevanta review-trådar är resolved och PR:n har mergats först efter att live-rulesetet faktiskt verkställt mergekontraktet ovan.
