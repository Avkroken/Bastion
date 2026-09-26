# Drift och verifiering

## Grundprincip

Bastion består av flera builddomäner. Kör verifiering för den kod som ändras och för delad kärna när gränssnitt eller dependency påverkas.

## Swift-kärna

Från repositoryts root:

```bash
swift test
```

Det verifierar `SSHCore`, `bastion-cli` och `SSHCoreTests`.

Vid dependencyändringar bör även package resolution/build verifieras från ren state.

## Apple

Generera projekt genom repositoryts wrapper:

```bash
sh App/generate-project.sh
```

Använd inte direkt XcodeGen-spec som ersättning när wrappern behövs för att mata dependencyversioner från `App/Package.swift`.

Verifiera berörd target separat:

- iOS: `Bastion`
- macOS: `Bastion-macOS`
- tvOS: `Bastion-tvOS`

### Apple-fel efter dependencyändring

Kontrollera i ordning:

1. `App/Package.swift`;
2. att `generate-project.sh` läser rätt version;
3. generated package mapping;
4. att `Package.swift` är exkluderad från app source;
5. target/scheme-specifik build.

## Android

Kör repositoryts Gradleverifiering från Androidprojektet med wrappern.

Vid dependency graph-fel, skilj mellan:

- buildscript `classpath`;
- appens runtime classpaths;
- dependency graph submission/snapshot.

Build-tool overrides ska inte flyttas till app-runtime enbart för att få en dependencycheck grön.

Om GitHub flaggar en sårbar build-tool dependency som endast är transitiv i den inskickade Gradle-grafen, verifiera först den faktiska resolutionen. Om parent-dependencyn inte kan uppgraderas till en säker transitiv version och Gradle/AGP stöder en override, deklarera den patchade dependency-versionen explicit på buildscript-classpathen. Det behåller säkerhetstäckningen och gör dependency maskinellt uppdateringsbar i stället för att filtrera bort den.

Efter en sådan ändring ska Android-builden vara grön och dependency graph-artifacten verifieras innehålla den patchade versionen. Default-branchens dependency submission är fail-closed och får inte göras best-effort för att dölja submissionsfel.

## Linux

Använd LinuxApp-projektets egen Rust-toolchain och manifest. Ändringar i delade protokollgränser ska dessutom verifiera Swift-kärnan om integrationen påverkas.

## Windows

Använd WindowsApp-projektets .NET-build/testflöde. Håll Windows-specifika buildproblem isolerade från rootens Swift package.

## CI-felsökning

Klassificera först felet som:

- Swift package,
- Apple/Xcode,
- Android/Gradle,
- Linux/Rust,
- Windows/.NET.

Ändra inte en annan plattforms dependency- eller buildmodell för att reparera ett isolerat fel.

## Säkerhetsinvariants

- credentials/private keys får inte läggas i repo eller logs;
- repo-specifika workflowvärden som används i shell ska citeras och helst föras via `env:`;
- dependency manifests ska fortsatt representera den build de påstås styra.

## Dokumentationsunderhåll

När en plattforms build-, manifest- eller generationmodell ändras ska motsvarande avsnitt uppdateras samtidigt. README ska förbli en kort karta och inte växa till en plattformshandbok.
