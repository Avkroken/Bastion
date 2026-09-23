# Drift och verifiering

## Grundprincip

Bastion har flera oberoende build-/testdomäner. Verifiera endast de domäner som ändringen berör under utveckling, men kör repositoryts required checks före merge.

## CI-domäner

Aktuell project-context beskriver repo-lokala workflows för:

- Swift
- Rust
- .NET
- Gradle

Apple application CI använder dessutom Xcode/XcodeGen och ska behålla Bastions dependency-wrapper.

## Apple-verifiering

När Apple-projektet genereras ska vägen gå via:

```text
sh App/generate-project.sh
```

Direkt generation från `App/project.yml` får inte ersätta detta utan motsvarande stöd för dependency-versionen från `App/Package.swift`.

Verifiera att:

- `Package.swift` inte kompileras som vanlig app source,
- iOS-, macOS- och tvOS-schemes fortfarande matchar project-context,
- dependency-versionen som Dependabot uppdaterar också används av den genererade appen.

## Central CI

När en lokal CI-domän flyttas till central org-CI:

1. behåll fungerande lokal gate,
2. verifiera central required-workflow mot en riktig Bastion-PR,
3. verifiera korrekt Custom Property/ruleset-selector,
4. bekräfta att rätt required check kommer från rätt source/path/ref,
5. ta först därefter bort redundant lokal implementation.

## Plattformsspecifik drift

Android, Linux och Windows ska verifieras med respektive etablerade buildsystem och repositoryts befintliga workflows. Introducera inte en ny parallell toolchain enbart för dokumentations- eller CI-bekvämlighet.

## Incidenter

Vid plattformsspecifikt CI-fel:

- isolera felet till rätt domän,
- verifiera att central policy och repo-profil fortfarande matchar,
- ändra inte Custom Properties/rulesets som workaround,
- bevara fungerande plattformar medan den felande domänen repareras.
