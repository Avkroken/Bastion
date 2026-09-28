# Release- och versionsstandard

**Senast verifierad:** 2026-09-28

Det här dokumentet gäller **Bastion-repositoryt**. Repositoryts egna dokument, manifests, workflows, taggar och GitHub Releases äger release- och versionskontraktet.

## Versionsankare

Bastions versionerade repositoryreleases använder:

```text
vMAJOR.MINOR.PATCH
```

Git-taggen och motsvarande GitHub Release är repositoryts kanoniska versionsankare.

Plattformarnas egna manifest- och buildversioner är separata. Exempelvis är Apple `MARKETING_VERSION` och andra plattformsspecifika versionsfält inte automatiskt repositoryts GitHub Release-version.

Inför inte `version.txt` eller en andra manuellt underhållen global versionskälla.

## PR-titlar och merge queue

Pull request-titlar ska följa Conventional Commits:

```text
<type>[optional scope][!]: <description>
```

Tillåtna typer är `feat`, `fix`, `perf`, `refactor`, `docs`, `test`, `build`, `ci`, `chore` och `revert`.

Scope är valfri och kan exempelvis vara `ssh`, `apple`, `android`, `linux`, `windows` eller `deps`.

`!` eller en `BREAKING CHANGE:`-footer markerar breaking change.

`.github/workflows/pr-title.yml` validerar titeln på pull request-event. På `merge_group` gör workflown en pass-through eftersom PR-titeln redan verifierats. Workflown använder inga secrets och har `permissions: {}`.

## SemVer

Automatisk versionsberäkning följer:

- breaking change → **major**;
- `feat` → **minor**;
- `fix`, `perf` och `revert` → **patch**;
- `refactor`, `docs`, `test`, `build`, `ci` och `chore` skapar normalt ingen release ensamma;
- `Release-As: major|minor|patch|none` får uttryckligen styra en icke-breaking ändring men får aldrig sänka en breaking change under major.

En plattformsspecifik intern buildräknare är inte en SemVer-release.

## Automatiskt releaseflöde

`.github/workflows/release.yml` äger den repo-lokala releaseprocessen.

Normal väg:

```text
PR
  -> Conventional Commit-kompatibel PR-titel
  -> plattformsspecifik CI/review
  -> merge till main
  -> samma main-SHA verifieras av push-workflows
  -> semantic release beräknar högsta nödvändiga bump
  -> immutable SemVer-tagg
  -> GitHub Release
```

En merge utan releasevärdig förändring skapar ingen release.

Releasejobbet:

- kör endast på `refs/heads/main`;
- serialiserar push- och manuella releasekörningar;
- använder full Git-historik och endast nåbara releaseankare;
- kräver de kontroller som anges i `.github/release-required-checks`;
- väntar på dessa checks på exakt release-target SHA;
- vägrar avancera från en SemVer-tagg som saknar motsvarande GitHub Release.

Ingen release skapas för att ”komma runt” CI, review eller repositoryskydd.

## Required checks

Bastions release-target ska ha lyckad verifiering från repositoryts plattforms- och säkerhetsdomäner:

- Rust;
- Android Gradle;
- Gradle dependency graph;
- .NET tests;
- Windows application;
- Swift package på Ubuntu;
- Swift package på macOS;
- Apple applications;
- CodeQL för Actions, C#, Java/Kotlin, Ruby, Rust och Swift.

Checknamnen versioneras i `.github/release-required-checks`. Releasegaten bedömer endast dessa uttryckligen required checks och kräver `success` för dem.

Dependency submission, Dependabot-automerge, wiki-sync eller andra event-/underhållsspecifika jobb kan vara kompletterande men får inte ersätta eller oavsiktligt blockera de obligatoriska release-checkarna.

## Release är inte distribution

GitHub Release, plattformsspecifik distribution och externa store-/packageflöden är separata händelser.

En SemVer-tagg får inte implicit börja publicera eller deploya Apple-, Android-, Linux- eller Windows-artefakter utan ett separat verifierat distributionskontrakt och dess least-privilege credentials.

## Prereleases

Manuell `workflow_dispatch` kan skapa release candidates:

```text
vMAJOR.MINOR.PATCH-rc.N
```

RC-sekvensen sorteras numeriskt. Om en starkare SemVer-förändring tillkommer efter en aktiv RC startas en ny RC-serie på den högre versionskärnan.

Promotion till stable ska alltid använda **samma commit som den aktiva RC-taggen**. En senare `main`-commit får inte följa med i stable-taggen utan att själv ha ingått i RC:n.

## Release notes

GitHub Releases är den officiella versionerade releasehistoriken.

Varje commit klassificeras i exakt en release-note-kategori. Breaking changes markeras som breaking men behåller sin relevanta grundkategori när sådan finns.

`.github/release.yml` kan fortsatt användas för GitHubs genererade release-note-kategorier, men den är inte releaseprocessen; automationen finns i `.github/workflows/release.yml`.

## Credentials och permissions

Releasejobbet använder repositoryts `GITHUB_TOKEN` med minsta nödvändiga permissions:

- `contents: write` för tagg och GitHub Release;
- `actions: read`, `checks: read` och `statuses: read` för verifieringsgaten.

Ingen ny PAT, ingen write-permission i read-only providerintegrationer och ingen bypass ska användas.

## Hotfix och rollback

Hotfix utgår normalt från aktuell `main` och använder `fix:` när förändringen är bakåtkompatibel.

Publicerade taggar flyttas eller skrivs inte om. Vid felaktig release:

1. korrigera eller revert:a via vanlig PR;
2. kör relevant plattformsverifiering;
3. mergea till `main`;
4. låt releaseprocessen skapa en ny korrigerande SemVer-version;
5. distribuera den korrigerade versionen endast i de kanaler där det behövs.

Ingen force-push eller tag history rewrite används.

## Verifiering

Vid ändring av releasekontraktet ska minst följande verifieras:

- PR-title-workflow på pull request och merge queue;
- SemVer-, RC- och breaking-logik;
- att release-target är nåbar från repositoryts versionshistorik;
- att samtliga required checks faktiskt körs på main-push;
- att release-target SHA är den SHA som checks verifierat;
- att RC-promotion pekar på aktiv RC-commit;
- att gamla misslyckade releasekörningar inte blockerar en senare lyckad recovery;
- att GitHub Release fortsatt är kanonisk versionshistorik.
