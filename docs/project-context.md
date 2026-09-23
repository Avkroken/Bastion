# Bastion project context

Det här dokumentet är Bastions levande, versionsstyrda projektkontext. Det ska bära sådant som annars lätt blir fel i chattar eller agentminne: plattformsindelning, CI-domäner, ruleset-kopplingar och viktiga bygginvarianter.

**Senast verifierad:** 2026-09-18

Organisationsgemensam styrning finns i:

- https://github.com/Avkroken/.github/blob/main/docs/engineering-context.md

Vid konflikt gäller live GitHub-inställningar och filer på `main` före det här dokumentet. Uppdatera sedan dokumentet så att konflikten inte ligger kvar.

## Repositoryts struktur

Bastion är ett multiplattformsrepository.

| Område | Plats | Roll |
| --- | --- | --- |
| Swift package / kärna | repository root, `Sources/`, `Tests/`, `Package.swift` | delad Swift-kod och SSHCore |
| Apple | `App/` | iOS, macOS och tvOS via XcodeGen |
| Android | `Android/` | Android/Gradle |
| Linux | `LinuxApp/` | Linux/Rust |
| Windows | `WindowsApp/` | Windows/.NET |

## Arbetsgrenar

Använd organisationens standard:

```text
{agent}/{feature}/{YYYY-MM-DD}/{HH-mm}-{id}
```

Ändringar görs i arbetsgren och öppnas som PR mot `main`.

## Custom Properties för Bastion

Bastions Custom Properties ska spegla vad repositoryt faktiskt innehåller.

### `ci_stack`

Avsedda stackvärden för Bastion:

- `swift`
- `rust`
- `dotnet`
- `gradle`

De motsvarar de separata bygg-/testdomänerna och de aktiva organisations-ruleseten:

- `main-swift`
- `main-rust`
- `main-dotnet`
- `main-gradle`

### `platform`

Avsedda plattformsvärden för Bastion:

- `apple`
- `windows`
- `linux`
- `android`

**Apple ska ligga i `platform`, inte i `ci_stack`.**

Den faktiska property-konfigurationen i GitHub-orgens UI är auktoritativ. Om UI-värden och den här filen skiljer sig ska skillnaden rättas och dokumenteras, inte lämnas som två konkurrerande sanningar.

## Rulesets

Verifierat på Bastion 2026-09-18:

- `main`
- `main-swift`
- `main-rust`
- `main-dotnet`
- `main-gradle`

`main-apple` är ännu inte en del av den verifierade aktiva listan.

### Beslutad Apple-selector

När Apple får eget required-workflow ruleset ska selector vara:

```text
platform = apple
```

Inte:

```text
ci_stack = apple
```

och inte en separat boolean som `ci_apple=true`.

## Nuvarande CI på `main`

Bastion har repo-lokala workflows för:

- `.github/workflows/swift.yml`
- `.github/workflows/rust.yml`
- `.github/workflows/dotnet.yml`
- `.github/workflows/gradle.yml`

Apple application CI ligger fortfarande i Bastions `swift.yml` på nuvarande `main`.

Organisationen har samtidigt centrala reusable implementations i `Avkroken/.github` för Swift, Apple, .NET, Gradle och Rust.

Målbilden är att SwiftPM och Apple/Xcode ska vara separata CI-domäner utan att duplicera implementationslogik mellan repositories.

Ta inte bort den nuvarande fungerande Apple-valideringen innan den nya `main-apple`-vägen är aktiv och verifierad mot Bastion.

## Apple/Xcode dependency invariant

Det här är en viktig bygginvariant och ska bevaras vid framtida CI- eller XcodeGen-ändringar.

### Dependabot-källa

`App/Package.swift` är den Dependabot-synliga manifestfilen för Apple-appens SwiftPM-beroenden.

Nuvarande SwiftTerm-pin:

```text
1.19.0
```

Manifestet ska finnas kvar även om Xcode-projektet genereras via XcodeGen.

### XcodeGen-versionen kommer från manifestet

`App/generate-project.sh`:

1. läser SwiftTerms `exact`-version från `App/Package.swift`,
2. exporterar `SWIFTTERM_VERSION`,
3. kör XcodeGen med `App/project.dependencies.yml`.

`App/project.dependencies.yml` inkluderar `project.yml` och ersätter package map så att SwiftTerm använder:

```yaml
exactVersion: "${SWIFTTERM_VERSION}"
```

Det gör `App/Package.swift` till den Dependabot-managed single source of truth för den faktiska version som det genererade Xcode-projektet använder.

### `Package.swift` får inte kompileras som app source

XcodeGen använder rekursiva source paths i iOS- och macOS-targeterna.

Därför måste `App/project.yml` exkludera:

```text
Package.swift
```

från både iOS- och macOS-targetens source-glob.

Orsak: om manifestet kompileras som vanlig app-källkod får Xcode bland annat:

```text
unable to resolve module dependency: 'PackageDescription'
```

Rätt fix är **inte** att ta bort manifestet. Rätt fix är att hålla manifestet utanför application target sources.

### Dependency review och Android build-tool graph

Bastions Android-build använder Android Gradle Plugin 9.4.1. AGP begär Bouncy Castle 1.80.2 transitivt på buildscript-classpathen, medan `Android/build.gradle.kts` tvingar `bcprov-jdk18on`, `bcpkix-jdk18on` och `bcutil-jdk18on` till 1.86.

GitHubs dependency snapshot-diff kan ändå exponera de ursprungligt begärda 1.80.2-noderna när base/head har olika snapshot-set. Avkrokens centrala Dependency Review-policy retry:ar snapshot warnings och har en Bastion-specifik exception för exakt tre GHSA-ID:n som är knutna till denna falskpositiva requested-version. Ingen package-, severity- eller repository-wide bypass används; övriga advisories fortsätter blockera.

Undantaget ska tas bort när AGP/dependency-snapshoten inte längre rapporterar den begärda 1.80.2-grafen.

## CI måste använda dependency-wrappern

När Apple-projektet genereras i CI ska dependency-versionen fortfarande matas från `App/Package.swift`.

För Bastions nuvarande lokala Apple-validering innebär det att projektgenereringen går genom:

```text
sh App/generate-project.sh
```

En framtida central Apple-workflow får inte ersätta detta med en direkt `xcodegen generate --spec App/project.yml` för Bastion utan motsvarande stöd för dependency-wrappern.

Annars blir `App/Package.swift` synlig för Dependabot men den uppdaterade versionen används inte av den byggda appen.

## Apple-targets

### iOS

Target:

```text
Bastion
```

i `App/project.yml`.

### macOS

Target:

```text
Bastion-macOS
```

### tvOS

Target:

```text
Bastion-tvOS
```

tvOS är en separat minimal app i `App/TVApp/` och använder inte SwiftTerm.

Apple-CI bör kunna visa plattformsspecifika fel separat så att iOS-, macOS- och tvOS-fel inte maskerar varandra.

## Centralisering: säker migrationsordning

När Bastion flyttas från lokala implementationer till central org-CI:

1. behåll fungerande lokal required CI,
2. skapa/verifiera central required-workflow entrypoint,
3. säkerställ att Bastion-specifika paths/schemes/dependency-wrapper stöds,
4. kör den centrala vägen mot en riktig Bastion-PR,
5. skapa/aktivera relevant property-selector och ruleset,
6. verifiera att required check kommer från rätt source/path/ref,
7. först därefter ta bort motsvarande lokala implementation.

För Apple ska `main-apple` väljas av `platform=apple`.

## Säkerhetsinvariant för Actions

Repo-specifika värden som skickas till reusable workflows får inte interpoleras osäkert direkt i shell-kommandon.

Använd `env:` + citerade shell-variabler när värden ska användas i `run:`.

Det centrala CI-lagret har redan behövt korrigeras för CodeQL workflow-input injection; återintroducera inte samma mönster lokalt.

## Uppdateringskontrakt

Uppdatera den här filen i samma PR när någon av följande saker ändras:

- Bastions `ci_stack`- eller `platform`-klassificering,
- aktiva Bastion-rulesets,
- vilket workflow som äger SwiftPM respektive Apple CI,
- Apple target/scheme-namn,
- dependency-wrappern,
- Dependabot-manifestets roll,
- XcodeGen package-source-modell,
- repositoryts plattformsstruktur,
- central-vs-lokal CI-arkitektur.

Ta bort föråldrad current-state-text när något ersätts. Använd Git-historiken för historik i stället för att låta dokumentet samla gamla konkurrerande sanningar.
