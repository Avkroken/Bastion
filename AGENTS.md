# AGENTS.md

Den här filen är repositoryts auktoritativa arbetsinstruktion för AI-agenter. Live GitHub-konfiguration är verkställande sanning: om dokumentation och faktisk enforcement skiljer sig ska live-regeln följas och mismatchen rapporteras konkret.

Läs även eventuell närmare `AGENTS.md` för filer som berörs samt `SKILLS.md`, som styr svarsformatet i detta repository.

## Arbetsprincip

Leverera fungerande, verifierade och avgränsade ändringar. Bevara befintlig arkitektur och repository-specifika konventioner om det inte finns ett konkret skäl att ändra dem. CI och review ska verifiera arbetet, inte vara första debuggern för fel som rimligen kan upptäckas före PR.

Innan implementation: läs relevant kod, tester, konfiguration och dokumentation; identifiera repositoryts faktiska build-, test-, lint- och CI-kommandon; gör minsta kompletta ändring; kontrollera att inga secrets, credentials, debugrester eller oavsiktliga filer läggs till.

## Brancher och pull requests

- Pusha aldrig direkt till `main`.
- Skapa en kortlivad arbetsgren för varje logisk ändring och öppna en ready PR mot `main` efter pre-PR-granskning.
- **Aktivera auto-merge omedelbart när PR:n skapats**, även om CI eller review fortfarande pågår.
- Direkt merge får endast användas om repositoryägaren uttryckligen begär det.
- Squash är den enda merge-metod som nuvarande `main`-ruleset tillåter.
- Repositoryt har för närvarande ingen aktiv merge queue.
- Kringgå aldrig branch protection, rulesets, required checks, reviews eller review-thread resolution.

De befintliga `platform/*`- och `core/swift`-brancherna har specialscope i `.github/workflows/scope-policy.yml`, men de utgör inte en obligatorisk branchpool för nytt arbete. `work/*` och `docs/content` ska inte återställas eller synkas automatiskt av repositoryautomation.

## Merge-gates och review

Required checks och olösta review-trådar är merge-blockerare. Alla review-kommentarer ska läsas och utvärderas; relevanta findings åtgärdas i samma PR. En tråd markeras resolved först när eventuell nödvändig fix är pushad och verifierad.

Efter varje ny commit eller push ska aktuell HEAD, required checks, mergeability, mergekonflikter, review-sammanfattningar och öppna/återöppnade review-trådar kontrolleras igen. När alla faktiska gates är gröna/lösta ska den redan armerade auto-merge-funktionen föra PR:n till `main`.

Om auto-merge inte sker trots gröna checks och lösta trådar ska exakt kvarvarande repositoryregel eller annan blockerare identifieras; forcera eller kringgå inte skydd.

## Faktisk CI-enforcement

Live-rulesetet `ci` kräver för närvarande exakt följande contexts på default branch:

- `android-build`
- `windowsapp-build`
- `windowsapp-core-tests`
- `linuxapp-build`
- `linuxapp-msrv`
- `swiftpm-linux`
- `swiftpm-macos`
- `xcodegen-and-build`

Detta är en känd mismatch mot CI-designen som infördes i PR #385, där avsikten var stabila aggregat såsom `CI / android`, `CI / windows`, `CI / linux`, `CI / swift-linux`, `CI / apple` och `CI / required`. Dokumentation ensam ändrar inte enforcement. Tills live-rulesetet uppdateras ska de åtta faktiska contexts ovan behandlas som merge-gates och mismatchen rapporteras; ändra inte CI för att fejka eller försvaga checks.

`.github/workflows/osv-scanner.yml`, paketerings-, release-, security- och deploy-workflows är kompletterande verifiering och är inte required contexts i nuvarande ruleset.

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

En PR-baserad uppgift är klar först när implementationen är färdig och avgränsad, relevanta tester/checks har körts eller en konkret begränsning dokumenterats, slutdiffen självgranskats, all review-feedback utvärderats, legitima findings åtgärdats, aktuell HEAD och required CI verifierats efter senaste commit, alla relevanta review-trådar är resolved och auto-merge har mergat PR:n eller fortfarande är armerad medan en verifierad obligatorisk gate väntar.
