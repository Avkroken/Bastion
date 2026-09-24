# Bastion project context

**Senast verifierad:** 2026-09-24

Detta dokument beskriver Bastions repo-specifika current-state: plattformsindelning, manifests och bygginvarianter. Organisationsgemensam GitHub-governance dokumenteras inte här.

## Repositorystruktur

| Område | Plats | Roll |
| --- | --- | --- |
| Swift package / kärna | root, `Sources/`, `Tests/`, `Package.swift` | `SSHCore` + `bastion-cli` |
| Apple | `App/` | iOS, macOS och tvOS via XcodeGen |
| Android | `Android/` | Android/Gradle |
| Linux | `LinuxApp/` | Linux/Rust |
| Windows | `WindowsApp/` | Windows/.NET |

## Root Swift package

Rootens `Package.swift` deklarerar:

- library `SSHCore`
- executable `bastion-cli`
- test target `SSHCoreTests`
- SwiftNIO SSH/NIO/Crypto dependencies.

Paketet är byggbart oberoende av Apple-appens XcodeGen-projekt.

## Apple dependency invariant

### Manifest

`App/Package.swift` är dependency-manifestet för Apple-appens SwiftPM-beroenden och ska finnas kvar som maskinläsbar source of truth för dependencyuppdateringar.

### Project generation

`App/generate-project.sh`:

1. läser relevant dependencyversion från manifestet;
2. exporterar den till XcodeGen;
3. genererar projektet med dependency-aware spec.

En direkt körning av `xcodegen generate --spec App/project.yml` är inte likvärdig om den hoppar över den kopplingen.

### Manifest som source

XcodeGen-targets får inte inkludera `App/Package.swift` som vanlig app-källkod. Om manifestet hamnar i source-globen försöker Xcode kompilera `PackageDescription` i apptargeten och builden bryts.

Rätt lösning är att exkludera manifestet från app-source, inte att ta bort manifestet.

## Apple-targets

Verifierade targetnamn:

- iOS: `Bastion`
- macOS: `Bastion-macOS`
- tvOS: `Bastion-tvOS`

tvOS-appen ligger separat i `App/TVApp/`.

## Android dependency graph

Android använder Gradle. Buildscript-classpath och runtime dependencies är separata dependencyytor.

Repositoryts Gradlekonfiguration innehåller explicita build-tool overrides för vissa transitiva dependencies. Dessa ska inte beskrivas som app-runtime dependencies om de endast gäller buildscript graph.

Dependency graph-generering ska omfatta de configurations som verifieringsflödet faktiskt behöver.

## Plattformsspecifika ändringar

En ändring i en plattformsapp ska inte antas vara neutral för övriga plattformar. Dokumentera och verifiera den plattform som ändras samt delad kärna om gränssnittet påverkas.

## Uppdateringskontrakt

Uppdatera dokumentet när:

- repositoryts plattformsstruktur ändras,
- Apple target/scheme eller generation ändras,
- dependency-wrapperns kontrakt ändras,
- manifests byter ansvar,
- delad kärna flyttas eller delas,
- Android build/dependency graph ändras materiellt.
