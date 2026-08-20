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

  **Varje required check är bunden till en app, inte bara till ett namn.**
  Varje rad har ett `integration_id` vid sidan av `context`, och kravet
  uppfylls bara av en check som postats av *den* appen. Pekar raden på fel
  app blir PR:en permanent blockerad medan checken syns grön i PR-vyn —
  det finns ingenting i gränssnittet som förklarar varför. Klarsprak satt
  fast på exakt det 2026-08-17: `CodeQL` med `integration_id: 15368` i
  stället för 57789. Fixen är att ta bort raden och lägga till den på nytt
  genom att välja checken ur förslagslistan, så bindningen blir rätt.
  Jämför gärna `integration_id` mellan repon vid nästa avstämning.
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
Protect main), per repo. Statusen 2026-08-17: båda reglerna är
urkryssade i samtliga repon.

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

### Avstämning mot alla sju repons ruleset 2026-08-20

Avläst via `GET /repos/blixten85/{repo}/rulesets`. Beskrivningen ovan är från
2026-08-17 och stämmer inte längre på fem punkter. Ta ställning till vilken
sida som är sanningen innan nästa repo synkas mot den här filen.

**1. `Block other branches` blockerar all grenskapelse — i alla sju repon.**
Rulesetet (`~ALL` minus `main` och `dev`, reglerna `creation`, `deletion`,
`non_fast_forward`) skapades 2026-08-19 med tom `bypass_actors`. Verifierat
genom att försöka pusha en ny gren till bastion:

```
remote: error: GH013: Repository rule violations found
remote: - Cannot create ref due to creations being restricted.
```

Ingen kan skapa en gren — varken repoägaren eller Dependabot. Befintliga
grenar går att uppdatera, men en raderad gren går inte att återskapa, och
`Automatically delete head branches` raderar dem efter varje merge. Fixen är
den texten redan föreskriver för "Dev": repo-admin (`RepositoryRole` 5) och
Dependabot (`Integration` 29110) som `bypass_actors` med `bypass_mode:
always`. Rulesetet heter numera `Block other branches`, inte `Dev`, och
undantar main och dev i stället för att täcka `~ALL` rakt av.

**2. `allowed_merge_methods` är `["merge", "squash", "rebase"]` i alla sju.**
Texten säger bara `merge`, "gäller alla repon", ändrat 2026-08-17. Antingen
gjordes ändringen aldrig, eller så är den återställd.

**3. `dismiss_stale_reviews_on_push` är `false` i alla sju.** Texten säger
`true`.

**4. `bypass_actors` är tom även i `Protect main`, i alla sju.** Texten säger
repo-admin (`RepositoryRole` 5, `always`).

**5. CodeQL är påslaget i alla sju**, både som required check och som
`code_scanning`-verktyg. Texten säger "på i bastion, av i övriga repon".

Om bindningen av required checks: de flesta rader saknar `integration_id` och
är alltså "Any source". De tre som är bundna är bundna rätt — klarsprak
`CodeQL` till 57789 (felbindningen till 15368 som beskrivs ovan är åtgärdad),
politiker-webapp `python` och `scan-pr / osv-scan` till 15368 (GitHub
Actions), product-describer `docker` till 15368.

`code_quality` finns kvar i **politiker-webapp** och ska bort — det är gaten
som väntar på en Copilot-analys som aldrig blir gjord. Övriga sex är rena.

## Required status checks — per check, per repo

Avläst via `GET /repos/blixten85/{repo}/rulesets/{id}` 2026-08-20. Alla
required checks nedan hör till rulesetet **Protect main**; `Block other
branches` har inga.

En check får bara vara required om den **rapporterar på varje PR mot main**.
Gör den inte det står kravet kvar som pending och PR:en går aldrig att merga,
utan att gränssnittet förklarar varför. Tre saker diskvalificerar:

1. **`if:` som hoppar över jobbet på PR.** `osv-scanner.yml` har `scan` med
   `if: github.event_name != 'pull_request'`. Jobbet rapporterar `skipped`,
   men den återanvända workflowen inuti kör aldrig — så det sammansatta
   namnet `scan / osv-scan` skapas överhuvudtaget inte på en PR.
2. **`paths:`-filter på workflowen.** Filtreras workflowen bort skapas ingen
   check. Gäller `android-build` i bastion (`Android/**`) och `image`/`docker`
   i politiker-webapp (`kontakter/scraper/**` m.fl.).
3. **Jobbet finns inte längre.** `typecheck (sender)`, `(campaign)` och
   `(healthcheck)` låste varje PR i politiker-webapp efter att de fyra Workers
   slogs ihop 2026-08-20.

Checkens namn är **jobbets id**, inte workflowens `name:`, och det är
versalkänsligt — `Python` matchar inte jobbet `python`. Matrisjobb heter
`jobb (värde)`. Ett jobb som anropar en återanvänd workflow heter
`anropande-jobb / anropat-jobb`. Välj alltid raden ur **Suggestions** i UI:t;
dyker sökordet bara upp under *Group newEntries* med "Any source" har GitHub
aldrig sett en check med det namnet — då är namnet fel, inte källan.

Kolumnen **Nu** är läget 2026-08-20: `har` = redan required, `lägg till` =
kör på varje PR men saknas, `nej` = kör men bör inte gata, `aldrig` = kan
aldrig uppfyllas.

### bastion

| Check | Kommer från | Nu | Varför |
|---|---|---|---|
| `xcodegen-and-build` | `xcode.yml` | har | Enda stället `App/` byggs |
| `swiftpm-macos` | `xcode.yml` | har | `swift test` mot SSHCore |
| `CodeQL` | default setup | har | Injektionskänsliga ytor: Docker-kommandobyggare, SSH-nyckelparser |
| `linuxapp-build` | `linuxapp-build.yml` | har | `cargo build` för LinuxApp |
| `swiftpm-linux` | `swiftpm-linux.yml` | **lägg till** | Kärnan på Linux-toolchain, kör på varje PR |
| `linuxapp-msrv` | `linuxapp-build.yml` | **lägg till** | Bryts av en beroendebump som höjer MSRV |
| `scan-pr / osv-scan` | `osv-scanner.yml` | **lägg till** | Differentiell — rapporterar bara det PR:en inför |
| `ios-screenshots` | `xcode.yml` | nej | Genererar artefakter, inte ett korrekthetstest |
| `build-deb` | `linux-packaging.yml` | nej | Paketering ska inte gata kodändringar |
| `build-rpm` | `linux-packaging-rpm.yml` | nej | Som ovan |
| `build-deb-linuxapp` | `linuxapp-packaging.yml` | nej | Som ovan |
| `build-rpm-linuxapp` | `linuxapp-packaging-rpm.yml` | nej | Som ovan |
| `android-build` | `android-build.yml` | aldrig | `paths: Android/**` |
| `scan`, `scan / osv-scan` | `osv-scanner.yml` | aldrig | Kör inte på PR |

### politiker-webapp

| Check | Kommer från | Nu | Varför |
|---|---|---|---|
| `typecheck (app)` | `ci.yml` | har | Matrisen har en post sedan sammanslagningen; namnet behålls för den här radens skull |
| `python` | `ci.yml` | har | Bunden till 15368 (GitHub Actions) |
| `CodeQL` | default setup | har | |
| `scan-pr / osv-scan` | `osv-scanner.yml` | har | Bunden till 15368 |
| `image`, `docker` | `docker.yml` | aldrig | `paths`-filtrerad till `kontakter/scraper/**`, `.github/actions/trivy/**` |
| `scan`, `scan / osv-scan` | `osv-scanner.yml` | aldrig | Kör inte på PR |

Enda repot med fullständig lista. Två saker ska däremot **bort**:

- **`code_quality`-regeln är tillbaka.** Det är gaten som kräver en
  Copilot-analys som aldrig blir gjord utan abonnemang — se avsnittet om
  Copilot-beroende regler ovan.
- **`Trivy` och `osv-scanner` står som required tools** under
  `code_scanning`, vid sidan av CodeQL. Trivy körs bara via `docker.yml`, som
  är `paths`-filtrerad — en PR som bara rör `app/` producerar inga
  Trivy-resultat. Övriga sex repon har bara CodeQL där.

### klarsprak

| Check | Kommer från | Nu | Varför |
|---|---|---|---|
| `CodeQL` | default setup | har | Bunden till 57789 — den felbindning som beskrivs ovan är åtgärdad |
| `scan-pr / osv-scan` | `osv-scanner.yml` | **lägg till** | |
| `scan`, `scan / osv-scan` | `osv-scanner.yml` | aldrig | Kör inte på PR |

**Repot har ingen `ci.yml`.** Workflowsen är `deploy.yml`, `osv-scanner.yml`,
`auto-assign.yml` och `dependabot-auto-merge.yml`. Workern är TypeScript men
ingenting typkontrolleras på en PR. Det är en lucka jämfört med övriga repon,
inte ett val.

### docker-idempotent-update

| Check | Kommer från | Nu | Varför |
|---|---|---|---|
| `lint` | `ci.yml` | har | |
| `CodeQL` | default setup | har | |
| `python` | `ci.yml` | **lägg till** | Kör på varje PR, saknas i rulesetet |
| `docker` | `docker.yml` | **lägg till** | Inget `paths`-filter — bygger imagen på varje PR |
| `scan-pr / osv-scan` | `osv-scanner.yml` | **lägg till** | |
| `scan`, `scan / osv-scan` | `osv-scanner.yml` | aldrig | Kör inte på PR |

### routines-relay

| Check | Kommer från | Nu | Varför |
|---|---|---|---|
| `repository-checks` | `ci.yml` | har | Repots enda CI-jobb |
| `CodeQL` | default setup | har | |
| `scan-pr / osv-scan` | `osv-scanner.yml` | **lägg till** | |
| `scan`, `scan / osv-scan` | `osv-scanner.yml` | aldrig | Kör inte på PR |

### product-describer

| Check | Kommer från | Nu | Varför |
|---|---|---|---|
| `python` | `ci.yml` | har | |
| `dependency-review` | `dependency-review.yml` | har | |
| `CodeQL` | default setup | har | |
| `docker` | `docker.yml` | har | Bunden till 15368. Summeringsjobb: `if: always()`, `needs: [image]`, failar om någon matrisgren failat |
| `node (app)` | `ci.yml` (matris) | **lägg till** | |
| `node (engine)` | `ci.yml` (matris) | **lägg till** | |
| `node (processor)` | `ci.yml` (matris) | **lägg till** | |
| `scan-pr / osv-scan` | `osv-scanner.yml` | **lägg till** | |
| `image (…)` | `docker.yml` (matris) | nej | `docker` täcker dem redan, utan matrisens namngivning |
| `scan`, `scan / osv-scan` | `osv-scanner.yml` | aldrig | Kör inte på PR |

### pastebinit

| Check | Kommer från | Nu | Varför |
|---|---|---|---|
| `python` | `ci.yml` | har | Repots enda CI-jobb |
| `CodeQL` | default setup | har | |
| `scan-pr / osv-scan` | `osv-scanner.yml` | **lägg till** | |
| `scan`, `scan / osv-scan` | `osv-scanner.yml` | aldrig | Kör inte på PR |

### Aldrig required, i något repo

- `github-advanced-security` — Copilot-checken. Alltid röd utan abonnemang.
- `osv-scanner` — code scanning-resultatet, inte jobbet. Hör hemma under
  *Require code scanning results*, inte bland status checks.
- `assign`, `auto-merge` — automatiseringsjobb. De utför något, de
  verifierar ingenting.
- Deploy-jobb (`deploy.yml`, `deploy-cloudflare.yml`) — kör på main, inte PR.

## Vilka repon guldstandarden gäller

Sju repon: `bastion`, `klarsprak`, `docker-idempotent-update`,
`routines-relay`, `politiker-webapp`, `product-describer`, `pastebinit`.

**`bastion-certificates` står medvetet utanför.** Det är ett privat
fastlane match-repo — sju filer, inga workflows, inga beroenden och ingen
kod. Varken `AGENTS.md`, `dependabot.yml`, osv-scanner eller CI hör hemma
där, och avsaknaden är alltså inte en lucka att täppa till vid nästa
inventering. Skulle repot någon gång få kod eller beroenden är det den
här raden som ska omprövas först.

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
