# REPO.md

`Bastion` är ett flerplattformsprojekt med Android-, Windows-, Linux-, Swift- och Apple-delar.

## Plattformsvalidering

- Android: Gradle build/test.
- Windows: .NET core-tester och WinUI-build.
- Linux: Rust/GTK-build, tester och MSRV-build.
- Swift Linux: Swift build/test i Linux-containern.
- Apple: iOS-, macOS- och tvOS-build samt Swift package build/test.
- `scope-policy` gäller endast de uttryckligen namngivna `platform/*`- och `core/swift`-grenarna.
- OSV används för beroendeskanning.

Packaging- och TestFlight-workflows är releasevalidering och ska inte blandas ihop med plattformsvalideringen ovan.

## GitHub-styrning

- Kanonisk arbets- och reviewpolicy finns i `Avkroken/.github/AGENTS.md`.
- `main` skyddas av det ärvda organisationsrulesetet `main` och repo-rulesetet `required-ci`.
- Required checks på `main` är `CI / android`, `CI / windows`, `CI / linux`, `CI / swift-linux`, `CI / apple` och `scope-policy`.
- `dev` är integrationsgren när ett aktivt `dev-pilot`-ruleset finns. Lägg inte required checks på `dev` förrän motsvarande workflows bevisligen producerar exakt dessa check-namn för PR mot `dev`.
- Organisationens CodeRabbit-UI är baslinje. Om `.coderabbit.yaml` finns i förrådet är den en lokal override; gemensamma auto-review- och gatevärden ska inte dupliceras där. Repo-specifik SSH/LinuxApp-granskningsvägledning får vara lokal om den uttryckligen behövs.

## Validering

Kör relevanta build- och testkommandon för den plattform som ändras.
