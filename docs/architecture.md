# Arkitektur

**Senast verifierad mot project-context:** 2026-09-23

## Översikt

Bastion är ett multiplattformsrepository där plattformarna delar domän-/kärnlogik men har separata buildsystem och applikationsprojekt.

```text
                Shared repository
                      |
      +---------------+----------------+
      |               |                |
 Swift package      App/            Platform apps
 Sources/Tests      XcodeGen        Android / Linux / Windows
      |               |                |
   SwiftPM       iOS/macOS/tvOS    Gradle / Rust / .NET
```

## Komponentgränser

### Delad Swift-kärna

Repository root, `Sources/`, `Tests/` och `Package.swift` innehåller Swift package-/kärnlogik.

### Apple

`App/` innehåller iOS-, macOS- och tvOS-targets. Xcode-projektet genereras med XcodeGen.

Den kritiska dependencykedjan är:

```text
App/Package.swift
      |
      | läses av
      v
App/generate-project.sh
      |
      | SWIFTTERM_VERSION
      v
App/project.dependencies.yml
      |
      v
XcodeGen / generated project
```

Det innebär att `App/Package.swift` både är Dependabot-synlig manifestkälla och source of truth för den version som den genererade appen använder.

### Android

`Android/` är Gradle-domänen.

### Linux

`LinuxApp/` är Rust-domänen.

### Windows

`WindowsApp/` är .NET-domänen.

## CI-/policygräns

Avkrokens centrala rulesets väljer stackar och plattformar genom Custom Properties. Repo-lokala workflows och central CI får inte skapa dubbla konkurrerande required checks för samma domän utan en avsiktlig migrationsplan.

## Säkerhetsinvariant

Repo-specifika values som når shell i reusable workflows ska passera via `env:` och citerade shell-variabler. Interpolera inte osäkra workflow-inputs direkt i `run:`.

## Dokumentationskälla

Detaljer om aktuella Custom Properties, rulesets, schemes och migrationsordning finns i [project-context.md](project-context.md). Den filen har företräde framför denna arkitektursammanfattning när current-state ändras.
