# Arkitektur

## Översikt

Bastion delar transport-/SSH-logik men håller UI- och plattformsintegration separat.

```text
                 +----------------+
                 |    SSHCore     |
                 | SwiftNIO SSH   |
                 +-------+--------+
                         |
          +--------------+---------------+
          |              |               |
    bastion-cli       Apple UI       plattformsspecifika appar
       Swift          Swift/Xcode     Android / Linux / Windows
```

## Delad Swift-kärna

Rootpaketet producerar `SSHCore` och `bastion-cli`.

`SSHCore` ska innehålla den delade SSH-/transportlogik som kan byggas utan plattformsspecifikt UI.

Det gör att kärnans tester kan köras med Swift Package Manager oberoende av Xcode, Android, Rust eller .NET.

## Apple

`App/` är en separat applikationsyta ovanpå delad logik.

Xcode-projektet genereras med XcodeGen. Dependency-versioner som hanteras via `App/Package.swift` måste gå igenom `App/generate-project.sh` så att manifestet och genererat projekt inte driver isär.

## Android

`Android/` är en Gradle-domän. Buildscriptdependencies och appens runtime dependencies har olika ansvar och ska hållas isär i dokumentation och dependency review.

## Linux

`LinuxApp/` är ett eget paket/projekt och ska inte pressas in i rootens Swift package-definition bara för att repositoryt är gemensamt.

## Windows

`WindowsApp/` har .NET-baserad buildyta och ska kunna verifieras separat från Swift/Apple.

## Dependency boundaries

- root `Package.swift` — Swift-kärna/CLI;
- `App/Package.swift` — Apple dependency automation/source;
- Android Gradlefiler — Android/build-tool graph;
- Linux/Windows manifests — respektive plattform.

En dependencyuppdatering ska göras i manifestet som faktiskt äger dependencyförhållandet.

## Säkerhetsgräns

SSH-kärnan hanterar säkerhetskänslig transportlogik. UI-lager ska inte kringgå kärnans host-/transportvalidering genom att implementera parallella osäkra genvägar.

Secrets, privata nycklar och credentials ska aldrig hårdkodas i plattformsprojekt eller testdata.

## Dokumentationsgräns

Denna fil beskriver repositoryts kodarkitektur. Organisationsgemensam CI/governance hör hemma i central organisationsdokumentation, inte här.
