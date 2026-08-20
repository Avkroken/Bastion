# CI och branchflöde

## Branchmodell

Repositoryt använder endast `dev` och `main`.

1. Arbete görs på `dev`.
2. PR öppnas från `dev` till `main`.
3. PR-CI verifierar ändringen.
4. Auto-merge får merga när required checks är gröna.
5. Efter uppdatering av `main` fast-forwardar `.github/workflows/sync-dev.yml` automatiskt `dev` till `main`.
6. Synken force-pushar aldrig och avbryter om `dev` har omergade commits.

Vanlig CI ska inte köras både som `push` till `dev` och som `pull_request` för samma commit. Plattform-CI körs därför på PR mot `main` och på push till `main`.

## Impact-routing

`.github/scripts/ci-impact.sh` klassificerar den faktiska diffen till Apple, root-Swift/SSHCore, Android, Windows, LinuxApp och CLI-paketering.

Principer:

- Apple UI/projekt => Apple-jobb.
- `Sources/SSHCore/**` => Apple + root-Swift + CLI-paketering.
- `Sources/bastion-cli/**` => root-Swift + CLI-paketering, inte Apple UI.
- `Tests/SSHCoreTests/**` => root-Swift.
- `Android/**`, `WindowsApp/**`, `LinuxApp/**` => respektive plattform.
- Gemensam protokollspec => alla berörda plattformar.
- Dokumentation/processmetadata => inga plattformsbyggen.
- Okänd kod/config eller ändring i själva impact-motorn => full matris (fail-open).

Required checks får inte filtreras bort på workflow-nivå med `paths:` eftersom GitHub då kan lämna dem permanent `Expected/Pending`. Required workflows startar därför ett billigt impact-jobb och använder job-level `if:` för att hoppa över irrelevant dyrt arbete. Icke-required workflows kan använda `paths:` direkt.

Routingtabellen testas av `.github/scripts/test-ci-impact.sh` och `.github/workflows/ci-impact-test.yml`.

## Mål

CI ska verifiera den kod som faktiskt kan påverkas, inte skjuta hela plattformsmatrisen på varje ändring. Vid osäkerhet prioriteras säkerhet framför besparing: kör mer CI i stället för att riskera en falsk negativ impact-bedömning.