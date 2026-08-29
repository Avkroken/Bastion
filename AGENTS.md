# AGENTS.md

Den här filen innehåller instruktioner för AI-agenter som arbetar i repositoryt.

Root-`AGENTS.md` är den auktoritativa källan för repositoryövergripande agentpolicy. En mer specifik `AGENTS.md` längre ned i katalogträdet får lägga till regler för sitt subtree, men ska inte duplicera eller motsäga den repositoryövergripande policyn.

Följ dessutom de repository-specifika instruktionerna längre ned i denna fil.

<!-- AVKROKEN-COMMON:START -->

## Arbetsprincip

Leverera fungerande, verifierade och avgränsade ändringar. CI, GitHub Copilot Code Review och mänskliga reviewers är oberoende verifieringslager och ska inte vara den första debuggern för fel som agenten rimligen kan upptäcka själv före en pull request. Ändra inte mer än uppgiften kräver och bevara befintlig arkitektur och repository-specifika konventioner om det inte finns ett konkret skäl att ändra dem.

## Innan implementation

1. Läs denna fil och eventuell närmare `AGENTS.md` för de filer som berörs.
2. Läs relevant implementation, tester, konfiguration och närliggande dokumentation innan lösningen bestäms.
3. Identifiera repositoryts faktiska build-, test-, lint-, typecheck- och CI-kommandon från befintlig konfiguration.
4. Följ repositoryts branchmodell. Skapa inte egna branchkonventioner och anta inte att en policy är ruleset-enforced utan att den faktiskt är det.
5. Gör minsta kompletta ändring som löser problemet.

## Pre-PR quality gate

Innan en ready pull request skapas eller uppdateras ska agenten granska hela den egna diffen mot PR:ns base branch, kontrollera korrekthet, säkerhet, felhantering, kompatibilitet och relevanta edge cases, köra alla relevanta lokala tester samt tillämplig lint/typecheck/build, lägga till eller uppdatera tester när beteende ändras och detta är praktiskt testbart, kontrollera att inga secrets/credentials/debugrester/oavsiktliga filer har lagts till och fixa legitima egna findings före extern review.

Efter en senare commit eller push ska påverkad validering köras igen. Om full lokal validering inte är möjlig ska detta redovisas konkret i PR:n; hitta inte på ett grönt resultat.

## Review-signal

Prioritera funktionell och teknisk signal framför redaktionell puts. Rapportera inte rena stavnings-, grammatik-, interpunktions-, wording- eller stilfel i README, dokumentation, Markdown, kodkommentarer, docstrings eller annan mänskligt läsbar prosa. Rapportera däremot ett textfel när det materiellt kan ändra teknisk betydelse, säkerhet, korrekthet, användarbeteende eller en instruktion som förväntas köras eller kopieras bokstavligt, samt typos i maskin- eller semantikbärande innehåll såsom identifierare, strängkonstanter, paths, konfigurationsnycklar, environment-variabler, API-fält, kommandon, flags, selectors, protokoll- och enumvärden.

Prioritera korrekthet, säkerhet, tillförlitlighet, kompatibilitet, tester och underhållbarhet.

## Reviewnivå och eskalering

Använd lägsta reviewnivå som ger tillräcklig säkerhet.

- **Low:** GitHub Copilot Code Review Lite för rutinmässiga, lokala och väl avgränsade ändringar.
- **Medium:** Copilot Balanced för icke-trivial logik, flera sammanhängande komponenter eller API-/kompatibilitetsbeteende.
- **High:** minst Balanced för auth/access control, credentials/secrets, persistent data/schema/migrationer, concurrency/retries/idempotency, distributed/cross-service state, protokoll/integrationskontrakt, releaseflöden, privilegierad infrastruktur eller stora/riskfyllda refactors. Om frågan fortfarande kräver en separat djup implementation eller ett oberoende andra pass, delegera via den installerade OpenAI Codex-agentens faktiska GitHub-`@handle`.
- **Critical:** Balanced + Codex när ett fel trovärdigt kan innebära auth bypass, secret exposure, dataförlust/-korruption, destruktiv/irreversibel migration eller annan exceptionell produktionspåverkan. Om frågan fortfarande är olöst eller väsentligt tvetydig, begär ett separat andra pass från den installerade Anthropic Claude-agentens faktiska GitHub-`@handle`.

Bygg inte ett nytt router-workflow enbart för denna eskalering. Native GitHub-delegering är standardvägen så länge inget organisationsbeslut säger annat.

## Pull request och merge

Pusha aldrig direkt till `main`. Följ repositoryts specifika branchmodell och skapa en ready PR först när pre-PR-gaten är genomförd.

Efter varje ny commit eller push ska den aktuella PR-statusen verifieras igen: aktuell HEAD, required checks/CI, mergeability, mergekonflikter och obligatoriska review-trådar/blockers.

När GitHub bedömer PR:n som direkt mergebar och alla tillämpliga repository-gates är uppfyllda — required checks/CI är klara och godkända, inga mergekonflikter finns och inga relevanta obligatoriskt olösta review-trådar eller andra blockers återstår — ska PR:n mergas omedelbart.

Försök inte aktivera auto-merge på en PR som redan är direkt mergebar. Använd auto-merge när PR:n ännu inte kan mergas enbart därför att obligatoriska gates fortfarande väntar och repositoryt stöder auto-merge. Repositoryts aktuella ruleset, merge queue och repositoryinställningar bestämmer vilka merge-metoder som är tillåtna. Om GitHub inte tillåter merge trots att PR:n ser grön ut ska den konkreta blockeraren identifieras; forcera eller kringgå inte repositoryskydd.

## Credentials och AI-infrastruktur

Committa eller exponera aldrig secrets, tokens, privata nycklar eller andra credentials. Lägg inte till `OPENAI_API_KEY`, `ANTHROPIC_API_KEY` eller annan extern AI-provider-credential i repository, Actions secrets eller organisationskonfiguration utan uttryckligt godkännande från repository- eller organisationsägaren. Ändra inte billing, Copilot-policy, repository secrets eller organisationsinställningar enbart för att möjliggöra AI-routing utan uttryckligt godkännande. Föredra befintliga GitHub/Copilot-native mekanismer framför nya workflows, botar eller dispatchers när de redan löser uppgiften.

## Definition of done

En agentuppgift är inte klar förrän implementationen är färdig och avgränsad, relevanta tester/checks har körts eller en konkret begränsning dokumenterats, den slutliga diffen självgranskats, legitima review-findings åtgärdats, PR-status verifierats mot aktuell HEAD, PR:n antingen mergats därför att alla gates är uppfyllda eller har auto-merge aktiverat därför att endast väntande obligatoriska gates återstår, och ingen repositoryregel har kringgåtts.

<!-- AVKROKEN-COMMON:END -->

## Repository-specifika instruktioner

### Branchmodell

Skapa en kortlivad arbetsgren för varje logisk ändring och öppna PR mot `main`. Kringgå aldrig branch protection, rulesets, required status checks, reviews, review-thread resolution, merge queues eller force-push restrictions.

### CI och validering

Använd repositoryts befintliga scripts och tooling. Inspektera `package.json`, taskfiler, scripts och dokumentation innan kommandon väljs. Required check-namn ska matcha GitHubs faktiska check contexts exakt. Ersätt inte repository-specifik CI med en generisk workflow enbart för att uppfylla governance och försvaga inte validering för att göra en PR mergebar.

### Säkerhet

Använd repositoryts etablerade secret-management- och environment-variable-mönster. Validera opålitlig extern input vid lämpliga boundaries och upprätthåll auth/authz server-side där det är relevant. Försvaga inte säkerhetskontroller för att få tester, builds eller deployments gröna.

### UI och design

För ändringar som berör UI, components, pages, styling eller layout ska `DESIGN.md` läsas först när filen finns. Återanvänd design tokens/components, bevara semantisk HTML och keyboard accessibility, säkerställ focus states/accessibility names och använd inte enbart färg för att kommunicera state.

### Dependencies

Undvik nya dependencies när plattformen, ramverket eller en befintlig dependency redan löser behovet. Håll nödvändiga nya dependencies snävt avgränsade och motivera dem i PR:n.
