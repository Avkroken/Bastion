# Release- och versionsstandard

**Senast verifierad:** 2026-09-26

Det här dokumentet gäller **Avkroken/Bastion**. Repositoryts egna dokument, manifests, workflows, taggar och GitHub Releases äger release- och versionskontraktet.

## Nuvarande release-state

Bastion har publicerad GitHub Release-historik med SemVer-taggar. Senast verifierade publicerade release är `v0.24.1` från 2026-09-07.

Current `main` har:

- ingen aktiv Release Please-workflow;
- ingen `.release-please-manifest.json`;
- ingen `release-please-config.json`;
- ingen repoövergripande lokal versionsfil som motsvarar GitHub Release-versionen;
- `.github/release.yml` för GitHubs genererade release notes-kategorier.

Release Please användes historiskt men togs avsiktligt bort i mergad PR #486 (`chore: use GitHub-native release defaults`). Historiska `release-please--branches--main`-PR:er beskriver därför äldre automation, inte current-state.

Inför inte Release Please igen enbart därför att äldre releasehistorik har Release Please-format.

## Versionsankare

Repositoryts versionerade releases använder:

```text
vMAJOR.MINOR.PATCH
```

Git-taggen och motsvarande GitHub Release är repositoryts versionsankare.

Bastions plattformar har även egna manifest-/buildversioner. Exempelvis innehåller Apple-projektet `MARKETING_VERSION` och delprojekt kan ha egna packageversioner. De värdena är plattformsspecifika och ska inte automatiskt behandlas som repositoryts GitHub Release-version.

Inför inte `version.txt` eller en andra manuellt underhållen global versionskälla utan ett separat versionsarkitekturbeslut.

## PR-titlar och squash commits

Pull request-titlar ska följa Conventional Commits:

```text
<type>[optional scope][!]: <description>
```

Tillåtna typer:

- `feat` — ny funktion;
- `fix` — buggfix;
- `perf` — prestandaförändring;
- `refactor` — beteendebevarande omstrukturering;
- `docs` — dokumentation;
- `test` — tester;
- `build` — build-/paketeringssystem;
- `ci` — CI/CD;
- `chore` — underhåll utan produktfunktion;
- `revert` — återställning av tidigare förändring.

Scope är valfri och kan exempelvis vara `ssh`, `apple`, `android`, `linux`, `windows` eller `deps`.

`!` markerar breaking change:

```text
feat(ssh)!: replace host configuration contract
```

Workflow `.github/workflows/pr-title.yml` validerar titeln på `pull_request`. Den använder inga secrets, checkar inte ut kod och har `permissions: {}`.

## SemVer

Vid en repositoryrelease gäller som normal regel:

- breaking change → **major**;
- `feat` → **minor**;
- `fix` → **patch**;
- `docs`, `test`, `chore`, `ci` och `build` → normalt ingen versionshöjning ensamma;
- `perf` och `refactor` bedöms efter faktisk användar-/kompatibilitetseffekt.

En plattformsspecifik intern buildräknare är inte en SemVer-release.

## När en release ska ske

Release sker kuraterat, inte på varje merge.

En versionerad release är motiverad när exempelvis:

- användarsynlig funktionalitet är färdig för en officiell versionspunkt;
- en fix bör kunna refereras som stabil release;
- ett delat SSH-/konfigurations-/kompatibilitetskontrakt ändras;
- flera färdiga ändringar ska samlas till en begriplig repositoryrelease;
- en breaking förändring kräver ny major-version.

En release ska inte skapas enbart för dokumentation, CI- eller dependencyunderhåll om det saknas en faktisk releaseeffekt.

## Release är inte deployment eller distribution

GitHub Release, plattformsspecifik distribution och externa store-/packageflöden är separata händelser.

En tagg eller GitHub Release får inte implicit börja publicera eller deploya plattformsartefakter utan ett separat verifierat distributionskontrakt.

Detta är särskilt viktigt i ett multiplattformsrepo: Apple-, Android-, Linux- och Windows-distribution kan ha olika verktyg, credentials och releasekrav.

## Nuvarande releaseflöde

Current `main` har ingen aktiv releaseautomation. Det verifierade flödet är därför kuraterat:

```text
main changes
  -> Conventional Commit-kompatibla PR-titlar/squash commits
  -> ordinarie plattformsspecifik CI
  -> välj SemVer-version utifrån faktisk releaseeffekt
  -> skapa immutable vMAJOR.MINOR.PATCH-tagg
  -> skapa GitHub Release
  -> separat distribution/deployment när sådan faktiskt ska ske
```

`.github/release.yml` styr GitHubs genererade release notes-kategorier. Den skapar inte taggar eller Releases på egen hand.

## Verifiering vid release

Minst de repositorychecks som gäller den ändrade koden ska vara gröna innan releasepunkten skapas.

Bastions builddomäner verifieras enligt [operations.md](operations.md):

- Swift-kärna;
- Apple/Xcode;
- Android/Gradle;
- Linux/Rust;
- Windows/.NET.

Om en release påverkar delad kärna eller flera plattformar ska respektive berörda domäner verifieras. En releaseprocess får inte kringgå normala PR-checks eller repositoryskydd.

## Releaseautomation

Release Please-konfigurationen är medvetet borttagen från current `main`. Återinförande eller val av annan automation är ett separat arkitekturbeslut.

En framtida release-PR-modell måste bevara:

- normal CI/review på release-PR:n;
- least-privilege write-identitet;
- inga nya onödiga PAT:ar;
- ingen write-permission i read-only providerintegrationer;
- ingen koppling som automatiskt distribuerar plattformsartefakter enbart därför att en release-PR mergas.

Standard-`GITHUB_TOKEN`-beteende och efterföljande workflowtriggers måste verifieras mot aktuell GitHub-dokumentation innan automation införs.

## Changelog och release notes

GitHub Releases är den officiella versionerade releasehistoriken.

`.github/release.yml` är konfiguration för GitHubs genererade release notes och ska inte förväxlas med releaseautomation.

Inför inte en separat manuellt underhållen `CHANGELOG.md` som konkurrerande source of truth. Om en versionsstyrd changelog återinförs ska den genereras som del av samma releaseprocess.

## Prereleases

Prerelease används endast när det finns ett konkret test-/distributionsbehov, exempelvis:

```text
v1.0.0-rc.1
```

Prerelease-status ska markeras i GitHub Release och får inte tolkas som implicit produktionsdistribution.

## Hotfix och rollback

Hotfix utgår normalt från aktuell `main` och använder `fix:` när förändringen är bakåtkompatibel.

Publicerade taggar flyttas eller skrivs inte om. Vid felaktig release:

1. korrigera eller revert:a via vanlig PR;
2. kör relevant plattformsverifiering;
3. skapa en ny korrigerande SemVer-version;
4. skapa ny tagg och GitHub Release;
5. distribuera endast den korrigerade versionen i de kanaler där det faktiskt behövs.

Ingen force-push eller tag history rewrite används.
