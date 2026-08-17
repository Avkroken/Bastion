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
- `.github/dependabot.yml` — ekosystemen varierar med projektet, men
  strukturen är densamma: veckoschema, `assignees: ["blixten85"]` och en
  `groups`-post som slår ihop minor + patch. `github-actions` finns i alla
  repon.

## Workflows (`.github/workflows/`)

Två standardfiler finns i alla aktiva repon: `auto-assign.yml` och
`dependabot-auto-merge.yml`. Utöver dessa projektspecifika CI- och
deploy-workflows (`ci.yml`, `docker.yml`, `deploy*.yml` m.fl.) vars
job-namn refereras i branch-rulesetet nedan.

`dependabot-auto-merge.yml` armerar auto-merge på Dependabots PR:er med
`gh pr merge --auto --merge`. **Inte `--squash` eller `--rebase`** — de
metoderna är avstängda både på repo-nivå och i rulesetet, så kommandot
skulle avvisas och auto-merge tyst sluta fungera (så var det i alla repon
fram till 2026-08-17).

Avvikelse: bastion saknar `auto-assign.yml` och löser tilldelningen med
`assignees:` i `dependabot.yml` istället.

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
- `non_fast_forward`
- `deletion` (skydd mot borttagning av main)
- `bypass_actors`: repo-admin (`RepositoryRole` 5), `bypass_mode: always`

**Copilot-beroende regler ska inte ingå.** Det finns inget
Copilot-abonnemang på kontot, så följande två regler hör inte hemma i
rulesetet och ska tas bort där de fortfarande sitter kvar. De gör dock
olika saker, och bara den ena blockerar:

- `code_quality` (`severity: all`) — *"Require code quality results"*.
  **Detta är gaten.** En code quality-analys måste ha genomförts på PR:en
  innan den kan mergas. Analysen körs av Copilot, så när jobbet havererar
  blir den aldrig gjord och PR:en fastnar i väntan på något som inte
  kommer.
- `copilot_code_review` (`review_on_push`, `review_draft_pull_requests`)
  — *"Automatically request Copilot code review"*. **Blockerar inte.**
  Regeln är villkorad ("if the author has access to Copilot code review
  and their premium requests quota has not reached the limit") och
  begär bara en review; utan access händer ingenting. Den är däremot
  vad som genererar den röda `github-advanced-security`-checken.

Felet det handlar om: `CAPIError: 400 The requested model is not
supported` (`COPILOT_AGENT_MODEL: sweagent-capi:claude-opus-4.6`),
reproducerbart på flera commits i bastion 2026-08-17, PR #324. CodeQL
påverkas **inte** — det är gratis för publika repon och ligger kvar som
både `code_scanning`-regel och required check.

Rulesets går bara att ändra i UI:t (Settings → Rules → Rulesets →
Protect main), per repo. Statusen 2026-08-17: reglerna finns kvar i
bastions export och är inte borttagna någonstans än.

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
| Dependabot version updates (`.github/dependabot.yml`) | **på i alla repon** — Dependabot är standarden. Renovate är utfasat och används inte längre någonstans (bastion migrerade 2026-07-14, övriga repon senare). |
| Code scanning / CodeQL | **på i bastion, av i övriga repon** — bastion har ingen `codeql.yml`; CodeQL körs via GitHubs *default setup* och syns i rulesetet som den required check som kommer från integration 57789. Motiverat av injektionskänsliga ytor (Docker-kommandobyggare, SSH-nyckelparser). Inte utrullat på övriga repon än. |

## Dependabot

Standardlösningen för beroendeuppdateringar i alla repon:

- `.github/dependabot.yml` med ekosystemen som repot faktiskt använder
- `.github/workflows/dependabot-auto-merge.yml` som armerar auto-merge med
  `gh pr merge --auto --merge`

Renovate-appen ska **inte** vara installerad, och inget repo har
`renovate.json` kvar. Undantag: `bastion-certificates` (fastlane
match-repo, sju filer, inga workflows och inga beroenden) har varken
Dependabot-konfiguration eller workflows — det finns inget att uppdatera
där.

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
