# Dokumentation

Navigationssida för Bastion.

## Hitta rätt

| Om du arbetar med… | Läs |
| --- | --- |
| repositoryts plattformar och bygginvarianter | [Projektkontext](project-context.md) |
| delad kärna och plattformsgränser | [Arkitektur](architecture.md) |
| bygg/test för Swift, Apple, Android, Linux eller Windows | [Drift](operations.md) |
| Apple-prototyper | [prototyp/](prototyp/) |
| säkerhetsrapportering | [SECURITY.md](../SECURITY.md) |

## Plattformskarta

```text
                    SSHCore
                      |
      +---------------+----------------+
      |               |                |
 bastion-cli       Apple apps       other apps
   Swift           iOS/macOS/tvOS    Android
                                    Linux
                                    Windows
```

## Kodområden

- `Sources/` — delad Swift-kärna och CLI-kod.
- `Tests/` — Swift-package tester.
- `App/` — XcodeGen-baserade Apple-appar.
- `Android/` — Androidprojekt.
- `LinuxApp/` — separat Linuxpaket.
- `WindowsApp/` — Windowsapplikation.

## Viktiga läsvägar

### Apple/Xcode

Läs project-context och operations innan `Package.swift`, XcodeGen-spec eller generation scripts ändras.

### Swift-kärna

Läs root `Package.swift`, `Sources/` och `Tests/`.

### Android

Läs Androids Gradlekonfiguration och operations-dokumentets dependency-graph-gräns.

### Linux/Windows

Behandla respektive app som separat builddomän med egen toolchain, men låt gemensam protokoll-/SSH-logik ligga i avsedd delad kärna där arkitekturen medger det.

## Wiki

Om GitHub Wiki används kan den ge en klickbar plattformsindelad manual. Den ska spegla den versionsstyrda dokumentationen här; repo-specifik teknisk current-state ska inte bara finnas i Wiki.
