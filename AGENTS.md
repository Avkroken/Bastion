# AGENTS.md

## Läs först

1. [docs/index.md](docs/index.md) — dokumentationskarta.
2. [docs/project-context.md](docs/project-context.md) — plattformar och bygginvarianter.
3. [docs/architecture.md](docs/architecture.md) — komponentgränser.
4. [docs/operations.md](docs/operations.md) — verifieringskommandon och plattformsspecifik drift.

Repositoryts egna dokument, manifests, workflows och versionerade konfiguration är auktoritativa för Bastions tekniska arbete.

## Arbetsregler

- Utgå från aktuell default branch och arbeta i separat arbetsgren enligt `{agent}/{feature}/{date}`, där `date` skrivs som `YYYY-MM-DD`.
- Arbetet ska vara seriellt och semantiskt per repository: en arbetsgren/PR motsvarar en sammanhängande feature eller uppgift, och `feature`-delen ska beskriva arbetet semantiskt.
- Innan agenten påbörjar nästa uppgift i samma repository ska befintlig öppen arbetsgren, draft eller PR färdigställas genom relevanta checks, reviews och merge, eller uttryckligen avslutas/blockeras. Skapa inte tids-/ID-suffix eller parallella branchvarianter för att kringgå ett upptaget namn.
- Om `{agent}/{feature}/{date}` redan finns för uppgiften ska agenten fortsätta den befintliga arbetslinjen i stället för att skapa en ny.
- Commits ska använda Conventional Commits eller motsvarande tydlig typ, exempelvis `feat:`, `fix:`, `docs:`, `chore:`, `ci:` eller `test:`.
- Läs hela PR-review-state före merge, inklusive kommentarer och trådar som GitHub markerar som `outdated`; verifiera att grundproblemet faktiskt är löst.
- Bevara gränsen mellan delad `SSHCore` och plattformsspecifika UI/appar.
- Apple-projektgenerering ska gå via `App/generate-project.sh` när dependency-wrappern behövs.
- `App/Package.swift` får inte börja kompileras som vanlig app-source i Xcode-targets.
- Ändra plattformsspecifika build paths och manifests tillsammans med relevant dokumentation.
- Repo-specifika värden som når shell ska hanteras via `env:` och citerade variabler.
- Lägg aldrig secrets eller credentials i repository eller publik dokumentation.
- Extern GitHub-governance är provider-state. Anta inte organization-scope eller andra org-funktioner utan live-verifiering.
