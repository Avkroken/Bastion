# Ruleset-filer

En katalog per repo, två filer i varje. Importeras i respektive repo under
**Settings → Rules → Rulesets → New ruleset → Import a ruleset**. Finns
rulesetet redan: importera som nytt och radera det gamla — importen skapar
alltid ett nytt ruleset, den skriver inte över.

Filerna saknar `id`, `source` och tidsstämplar med flit. De fälten sätts av
GitHub vid import och gör filen repo-bunden om de ligger kvar.

## Vad filerna säger

**`block-other-branches.json`** — identisk i alla repon. `~ALL` utom
`refs/heads/main` och `refs/heads/dev`, med `creation`, `deletion` och
`non_fast_forward`. Två grenar, inget annat.

**`protect-main.json`** — skräddarsydd per repo. Gemensamt:

- PR krävs, noll obligatoriska godkännanden, trådar måste vara lösta
- `allowed_merge_methods: ["merge"]` — varken squash eller rebase
- `dismiss_stale_reviews_on_push: true`
- `strict_required_status_checks_policy: true` — grenen måste vara
  uppdaterad mot main innan merge
- `code_scanning` med `alerts_threshold` och `security_alerts_threshold`
  båda på `all`. Allt flaggas, inte bara critical/high/errors.

Skillnaden mellan repon är listan `required_status_checks` och vilka
code scanning-verktyg som faktiskt kör där. Underlaget står i
[`GULDSTANDARD.md`](../../GULDSTANDARD.md), en rad per check per repo.

## Två saker att veta innan import

**Dependabot slutar fungera.** `creation` på `~ALL` utan `bypass_actors`
hindrar Dependabot från att skapa sina `dependabot/*`-grenar, alltså från
att öppna PR:er. Det är en direkt följd av tvågrenskravet, inte ett
förbiseende. Vill du ha kvar Dependabot lägger du in det här i
`bypass_actors` i `block-other-branches.json`:

```json
"bypass_actors": [
  { "actor_id": 29110, "actor_type": "Integration", "bypass_mode": "always" }
]
```

**Ingen kan skapa en gren, inte heller du.** `Automatically delete head
branches` raderar grenen när en PR mergas. Är `dev` en gång raderad går den
inte att återskapa utan att först inaktivera rulesetet. Överväg att stänga av
den repo-inställningen, eller lägg repo-admin (`RepositoryRole` 5) i
`bypass_actors` som säkerhetsventil.

## Checkarnas bindning

Varje required check är bunden till appen som postar den — `integration_id`
15368 för GitHub Actions, 57789 för CodeQL. Bindningen är verifierad mot
kontots egna ruleset. Pekar en rad på fel app blir PR:en permanent blockerad
medan checken syns grön i PR-vyn.
