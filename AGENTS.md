# AGENTS.md

## Läs först

- [docs/project-context.md](docs/project-context.md) — canonical current-state för plattformar, CI, rulesets och bygginvarianter.
- [docs/architecture.md](docs/architecture.md) — komponentgränser.
- [docs/operations.md](docs/operations.md) — verifiering och migrationsordning.
- `Avkroken/.github/docs/engineering-context.md` — organisationsgemensam CI/governance.
- `Avkroken/.github/docs/documentation-standard.md` — dokumentationsmodell.

## Arbetsregler

- Arbeta i separat gren enligt `{agent}/{feature}/{YYYY-MM-DD}/{HH-mm}-{id}`.
- Bevara den plattformsindelning som dokumenteras i project-context.
- Apple-projektgenerering ska gå via `App/generate-project.sh` när dependency-wrappern behövs.
- Ta inte bort fungerande lokal CI innan motsvarande central gate är aktiv och verifierad.
- Ändra inte rulesets eller Custom Properties som workaround för failing checks.
- Repo-specifika workflow-värden som når shell ska hanteras via `env:` och citerade variabler.
- Lägg aldrig secrets eller credentials i repository eller publik dokumentation.
