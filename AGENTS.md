# AGENTS.md

Den här filen är repositoryts auktoritativa arbetsinstruktion. Live GitHub-konfiguration är verkställande sanning när dokumentation och faktisk enforcement skiljer sig. Läs även närmare `AGENTS.md` för berörda subtrees och `SKILLS.md` för svarsformat.

## Arbetsprincip

Gör minsta kompletta ändring. Läs relevant kod, tester, konfiguration och dokumentation före implementation. Kontrollera att inga secrets, credentials, debugrester eller oavsiktliga filer läggs till.

## Brancher och pull requests

- Pusha aldrig direkt till `main`.
- Använd en kortlivad branch och en ready PR mot `main`.
- Auto-merge får aktiveras först när aktuell HEAD uppfyller repositoryts verifierade gates.
- Endast squash merge är tillåtet.
- Kringgå aldrig rulesets, checks, reviews eller thread resolution.

## Merge-gates

Live organisationsrulesets kräver på senaste PR-HEAD:

- `CI / android`
- `CI / windows`
- `CI / linux`
- `CI / swift-linux`
- `CI / apple`
- `scope-policy`
- `scan-pr / osv-scan`

Statuspolicyn är strict. Org-rulesetet för `main` kräver dessutom en approval, avvisar stale reviews efter push, kräver last-push approval av någon annan, lösta review-trådar och CodeQL merge protection. Copilot och CodeRabbit är rådgivande men faktiska relevanta findings ska utvärderas och åtgärdas.

Efter varje push ska aktuell HEAD, required checks, mergeability och review-state kontrolleras igen.

## CI-design

Required plattformsworkflows ska vara direkta verifieringar, inte routinglager. Varje required check kör den verifiering som dess namn påstår:

- Android: Gradle build/test
- Windows: .NET core tests och WinUI build
- Linux: Rust/GTK build, tests och MSRV-build
- Swift Linux: Swift build/test i Linux-container
- Apple: iOS/macOS/tvOS builds samt Swift package build/test
- scope-policy: begränsar endast de explicit namngivna `platform/*`- och `core/swift`-brancherna
- OSV: repositoryts egen dependency scan

Paketerings- och TestFlight-workflows är produkt-/releaseverifiering och är separata från required merge-gates.

Repositoryts workflows får inte skapa eller uppdatera PR:er eller branches, arma eller genomföra merge, automatisera review, delegera arbete till AI-agenter eller lagra säkerhetsalert-snapshots. Security alerts hanteras av GitHubs native säkerhetsfunktioner och kodändringar går genom normala PR-gates.

GitHub Actions ska pinnas till full commit-SHA.

## Pre-PR quality gate

Granska hela diffen mot base branch. Kör tillämpliga tester, lint, typecheck och build. Lägg till eller uppdatera tester när beteende ändras och detta är praktiskt testbart. Om full lokal validering inte är möjlig ska begränsningen beskrivas konkret i PR:n.

## Säkerhet

Committa eller exponera aldrig secrets, tokens eller privata nycklar. Använd etablerad secret-management. Validera opålitlig input vid rätt boundary och försvaga inte säkerhetskontroller för att få tester eller builds gröna.

## UI och design

För UI-, component-, page-, styling- eller layoutändringar ska `DESIGN.md` läsas först när den finns. Bevara accessibility och återanvänd repositoryts designmönster.

## Dependencies

Undvik nya dependencies när plattformen eller befintliga dependencies redan löser behovet. Motivera nödvändiga nya dependencies.

## Definition of done

En PR-baserad uppgift är klar först när implementationen är avgränsad, relevanta tester/checks har körts eller en konkret begränsning dokumenterats, slutdiffen självgranskats, review-feedback utvärderats, legitima findings åtgärdats, aktuell HEAD och required gates verifierats och relevanta review-trådar är resolved.

## PR-scope efter öppning

- PR:ns avsedda scope är fryst efter öppning.
- Fel som orsakas av PR:ns befintliga ändringar rättas i samma PR.
- Ny funktionalitet, opportunistiska refactors och separata förbättringar får en ny branch/PR.
- Efter korrigerande commits ska relevanta tester samt gate- och review-state verifieras på den nya HEAD:en.
