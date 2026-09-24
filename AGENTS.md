# AGENTS.md

## Läs först

1. [docs/index.md](docs/index.md) — dokumentationskarta.
2. [docs/project-context.md](docs/project-context.md) — plattformar och bygginvarianter.
3. [docs/architecture.md](docs/architecture.md) — komponentgränser.
4. [docs/operations.md](docs/operations.md) — verifieringskommandon och plattformsspecifik drift.

## Arbetsregler

- Utgå från aktuell `main` och arbeta i separat arbetsgren.
- Bevara gränsen mellan delad `SSHCore` och plattformsspecifika UI/appar.
- Apple-projektgenerering ska gå via `App/generate-project.sh` när dependency-wrappern behövs.
- `App/Package.swift` får inte börja kompileras som vanlig app-source i Xcode-targets.
- Ändra plattformsspecifika build paths och manifests tillsammans med relevant dokumentation.
- Repo-specifika värden som når shell ska hanteras via `env:` och citerade variabler.
- Lägg aldrig secrets eller credentials i repository eller publik dokumentation.

Organisationsgemensam GitHub-policy dokumenteras centralt och ska inte dupliceras som Bastion-current-state här.
