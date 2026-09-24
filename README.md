# Bastion

Bastion är ett multiplattformsrepository med delad kärna och separata applikationsytor för Apple-plattformar, Android, Linux och Windows.

## Struktur

- **Swift package / kärna:** repository root, `Sources/`, `Tests/`, `Package.swift`
- **Apple:** `App/` för iOS, macOS och tvOS via XcodeGen
- **Android:** `Android/` via Gradle
- **Linux:** `LinuxApp/` via Rust
- **Windows:** `WindowsApp/` via .NET

## Dokumentation

- [Projektkontext](docs/project-context.md) — canonical current-state för plattformar, CI, rulesets och viktiga bygginvarianter
- [Arkitektur](docs/architecture.md)
- [Drift och verifiering](docs/operations.md)
- [Avkrokens dokumentationsstandard](https://github.com/Avkroken/.github/blob/main/docs/documentation-standard.md)

## Viktig Apple-invariant

`App/Package.swift` är Dependabot-synlig source of truth för SwiftPM-beroenden. Xcode-projektet genereras via `App/generate-project.sh`, som matar dependency-versioner vidare till XcodeGen. En direkt ersättning med `xcodegen generate --spec App/project.yml` kan därför bryta kopplingen mellan Dependabot och den faktiskt byggda appen.

## CI

Bastion omfattar flera separata CI-domäner: Swift, Rust, .NET, Gradle samt Apple/Xcode. Repositoryts aktuella [projektkontext](docs/project-context.md) beskriver vilka centrala rulesets och profiler som gäller.

## Issues och säkerhet

Använd GitHub Issues för reproducerbara fel eller förbättringsförslag. Rapportera sårbarheter privat enligt [SECURITY.md](SECURITY.md).
