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

## Validering

Kör relevanta build- och testkommandon för den plattform som ändras.
