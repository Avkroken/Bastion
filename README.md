# Bastion

Bastion är ett multiplattformsprojekt med en delad Swift-baserad SSH-kärna och separata applikationsytor för Apple-plattformar, Android, Linux och Windows.

## Struktur

| Område | Plats | Teknik |
| --- | --- | --- |
| delad SSH-kärna + CLI | `Sources/`, `Tests/`, `Package.swift` | Swift / SwiftNIO |
| iOS, macOS, tvOS | `App/` | Swift / XcodeGen |
| Android | `Android/` | Kotlin / Gradle |
| Linux | `LinuxApp/` | Rust |
| Windows | `WindowsApp/` | .NET |

## Snabb verifiering

Swift-kärnan:

```bash
swift test
```

Övriga plattformar har egna bygg- och verifieringsflöden; se **[dokumentationsöversikten](docs/index.md)**.

## Dokumentation

- [Dokumentationsöversikt](docs/index.md)
- [Projektkontext](docs/project-context.md) — plattformar och bygginvarianter
- [Arkitektur](docs/architecture.md) — komponentgränser och delad kärna
- [Drift och verifiering](docs/operations.md) — plattformsspecifika kontroller
- [Release- och versionsstandard](docs/release-standard.md) — SemVer, PR-titlar och releasegränser
- [SECURITY.md](SECURITY.md) — säkerhetsrapportering

## Viktig Apple-invariant

`App/Package.swift` är manifestet som dependency automation kan läsa. `App/generate-project.sh` läser dependencyversioner därifrån och matar dem vidare till XcodeGen. Generera därför inte Bastions Apple-projekt med en direkt `xcodegen generate --spec App/project.yml` om det kringgår wrappern.
