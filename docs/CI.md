# CI och branchflöde

## Branchmodell

`main` är den enda långlivade arbetsgrenen. Varje ändring görs på en kortlivad branch och går via PR till `main`. Auto-merge används inte och merge-metoden är squash.

CI körs på PR mot `main` och, där efter-merge-verifiering eller publicering behövs, på push till `main`. Kortlivade arbetsbrancher behöver ingen separat push-CI när samma commit redan verifieras av PR-eventet.

## Impact-routing

`.github/scripts/ci-impact.sh` klassificerar diffen till Apple, root-Swift/SSHCore, Android, Windows, LinuxApp och CLI-paketering.

- Apple UI/projekt => Apple-jobb.
- `Sources/SSHCore/**` => Apple + root-Swift + CLI-paketering.
- `Sources/bastion-cli/**` => root-Swift + CLI-paketering.
- `Tests/SSHCoreTests/**` => root-Swift.
- `Android/**`, `WindowsApp/**`, `LinuxApp/**` => respektive plattform.
- Okänd kod/config eller ändring i impact-motorn => full matris.

Required checks får inte filtreras bort på workflow-nivå med `paths:` om det kan lämna dem i `Expected/Pending`. Required workflows använder därför billiga impact-jobb och job-level `if:` för dyrt arbete. Routingtabellen testas av `.github/scripts/test-ci-impact.sh` och `.github/workflows/ci-impact-test.yml`.

Målet är att verifiera relevant kod utan att köra hela plattformsmatrisen i onödan. Vid osäkerhet körs mer CI, inte mindre.
