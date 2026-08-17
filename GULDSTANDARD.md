# Guldstandard

Den repokonfiguration som ska matcha alla blixten85-repon. Verifierat mot
`routines-relay`, `scraper`, `politiker-webapp`, `product-describer-cloudflare`
2026-07-04. Använd den här filen som checklista när ett nytt repo skapas
eller när du undrar "är X satt här också?".

Branch-rulesetsen nedan är avstämda mot bastions exporterade JSON 2026-08-17.

## Filer i repot

- `LICENSE` (MIT)
- `SECURITY.md`
- `AGENTS.md`
- `CLAUDE.md`
- `.github/pull_request_template.md`
- `.github/ISSUE_TEMPLATE/config.yml` + `bug_report.yml` + `feature_request.yml`
- `.github/labeler.yml`
- `.github/FUNDING.yml` (github-sponsors + PayPal)
- `.github/renovate.json` (`config:best-practices`, daglig schedule, semantic
  commits, separata major-releases, auto-rebase, patch-automerge, GHA-gruppering)
  — **inte i bastion** (använder `.github/dependabot.yml` istället, se
  *Security & analysis* nedan)

## Workflows (`.github/workflows/`)

8 standardfiler: `auto-commit.yml`, `auto-label.yml`, `auto-merge.yml`,
`auto-rebase.yml`, `auto-release.yml`, `ci-autofix.yml`,
`copilot-review-reminder.yml`, `security-alerts-sync.yml`.

Utöver dessa: projektspecifika CI-workflows (bygger/testar koden) vars
job-namn refereras i branch-rulesetet nedan.

## Branch-rulesets

Två rulesets, båda med target `branch` och `enforcement: active`. De ska se
likadana ut i alla repon — det enda som skiljer repon åt är vilka jobb som
listas under `required_status_checks` (kodanalyserna och de projektspecifika
CI-jobben).

### "Protect main" (`~DEFAULT_BRANCH`)

- `pull_request`: `required_approving_review_count: 0` (PR krävs, men inga
  obligatoriska godkännanden), `dismiss_stale_reviews_on_push: true`,
  `required_review_thread_resolution: true`, `require_code_owner_review: false`,
  `require_last_push_approval: false`,
  **`allowed_merge_methods: [merge]`** — bara merge-commits, varken squash
  eller rebase (gäller alla repon)
- `required_status_checks`: `strict_required_status_checks_policy: true`
  (grenen måste vara uppdaterad mot main innan merge),
  `do_not_enforce_on_create: false`. **Repospecifik lista** — för bastion:
  `xcodegen-and-build`, `swiftpm-macos`, `linuxapp-build` samt `CodeQL`
  (integration 57789).
- `code_scanning`: CodeQL med `alerts_threshold: all` och
  `security_alerts_threshold: all`
- `code_quality`: `severity: all`
- `copilot_code_review`: `review_on_push: true`,
  `review_draft_pull_requests: true`
- `non_fast_forward`
- `deletion` (skydd mot borttagning av main)
- `bypass_actors`: repo-admin (`RepositoryRole` 5), `bypass_mode: always`

### "Dev" (`~ALL`)

Täcker alla grenar, inklusive main:

- `creation`, `deletion`, `non_fast_forward`
- `bypass_actors` (`always`): repo-admin (`RepositoryRole` 5) och Dependabot
  (`Integration` 29110) — bypassen är det som gör att Dependabot kan skapa
  och ta bort sina PR-grenar trots `creation`/`deletion`.

Ingen tag-ruleset finns någonstans i org:et — release-taggar (`auto-release.yml`)
träffar aldrig rulesetsen eftersom båda har target `branch`.
"Release-immunitet" är alltså inget konfigurerat koncept, bara en konsekvens
av att taggar och grenar är olika saker.

## Repo-inställningar (Settings → General)

| Inställning | Värde | Källa |
|---|---|---|
| Issues | på | alla repon |
| Projects | på | alla repon |
| Wiki | på | alla repon |
| Discussions | **på** | alla repon (bastion saknade detta, fixat 2026-07-04) |
| Sponsorships | på (via `.github/FUNDING.yml`, ingen separat toggle) | alla repon |
| Template repository | **på** | alla repon (bastion saknade detta, fixat 2026-07-04) |
| Require contributors to sign off on web-based commits | **på** | alla repon (bastion saknade detta, fixat 2026-07-04) |
| Always suggest updating pull request branches | **på** | alla repon (bastion saknade detta, fixat 2026-07-04) |
| Allow auto-merge | på | alla repon |
| Automatically delete head branches | på | alla repon |
| Allow merge commits | på | alla repon |
| Allow squash merging / Allow rebase merging | **av** | alla repon (ändrat 2026-08-17). Repo-nivån speglar rulesetet: det som är påslaget här är exakt `allowed_merge_methods` i `Protect main`. |

## Security & analysis

| Inställning | Värde |
|---|---|
| Dependabot security updates | på |
| Secret scanning | på |
| Secret scanning push protection | på |
| Secret scanning validity checks | av |
| Secret scanning non-provider patterns | av |
| Dependabot version updates (`.github/dependabot.yml`) | **av i övriga repon, på i bastion** — Renovate används i övriga repon (se `renovate.json`); bastion migrerade till Dependabot 2026-07-14 (`.github/dependabot.yml`), medveten avvikelse. |
| Code scanning / CodeQL | **av i övriga repon, på i bastion** (`.github/workflows/codeql.yml`, tillagt 2026-07-04) — gratis för publika repon, motiverat av injektionskänsliga ytor (Docker-kommandobyggare, SSH-nyckelparser). Inte utrullat på övriga repon än, så bastion avviker medvetet här tills vidare. |

## Renovate (GitHub App)

Ska vara **installerad och den auto-genererade "Configure Renovate"-PR:n
mergad** (inte lämnad öppen) — det är mönstret i alla andra repon.

**Undantag: bastion** använder inte Renovate (migrerade till Dependabot
2026-07-14, se *Security & analysis* och `.github/dependabot.yml`). Renovate-appen behöver
inte vara installerad för bastion.

## Inte verifierat / inte en del av guldstandarden

Följande dök upp i GitHubs inställningssida men kunde inte verifieras
programmatiskt (inget REST/GraphQL-fält hittades) eller är inte satt någonstans:

- **Limit how many branches and tags can be updated in a single push** —
  inget API-fält hittat, ingen indikation att något repo ändrat från default.
- **Enable release immutability** — nyare GitHub-funktion, inte satt i något
  av de granskade reporna. Överväg separat om det blir relevant (låser
  publicerade releasers assets/taggar mot ändring).
- **Automatic dependency submission** — inget API-fält hittat för att
  verifiera programmatiskt; ingen dedikerad workflow för det i något repo.
- **Dependabot malware alerts** — verkar höra ihop med
  `dependabot_security_updates` (redan på, identiskt överallt), inget separat
  fält hittat.

Om du vill ha någon av dessa satta måste det göras manuellt via
repo-inställningssidan på github.com — jag har inte ett verktyg som kan
bekräfta eller ändra dem tillförlitligt.

## Övrigt verifierat (redan i linje, inget att fixa)

- **Repo-topics**: tomt överallt, inte en del av guldstandarden.
- **Actions default workflow permissions**: `read` + `can_approve_pull_request_reviews: false`
  identiskt överallt (workflows som behöver skrivrättigheter deklarerar det
  explicit i sin egen YAML, t.ex. `contents: write` i `auto-release.yml`).
- **Private vulnerability reporting**: på, identiskt överallt.
- **CONTRIBUTING.md / CODE_OF_CONDUCT.md**: inte universella — `scraper` har
  en egen `CONTRIBUTING.md`, `routines-relay`/bastion har det inte. Projekt-
  specifikt, inte en del av guldstandarden.
