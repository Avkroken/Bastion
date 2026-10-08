# Bidra till Avkroken

Tack för att du vill bidra till Avkrokens projekt. Den här guiden gäller som standard för Avkrokens publika repositories när ett repository inte har egna, mer specifika riktlinjer.

## Hitta rätt kanal

- **Buggar:** skapa ett issue med felrapportmallen och ange tydliga steg för att återskapa problemet.
- **Konkreta förbättringsförslag:** använd feature request-mallen.
- **Frågor, tidiga idéer och öppna resonemang:** använd Avkrokens GitHub Discussions.
- **Säkerhetsproblem:** följ `SECURITY.md`. Publicera inte sårbarhetsdetaljer i ett publikt issue, en pull request eller en discussion.

## Arbetsflöde

1. Utgå från repositoryts aktuella default branch.
2. Gör ändringen i en separat arbetsgren enligt `{agent}/{feature}/{date}`, där `date` skrivs som `YYYY-MM-DD`.
3. Håll ändringen så liten, semantisk och fokuserad som möjligt.
4. Kör relevanta tester, linting, byggsteg och andra kontroller som finns i repositoryt.
5. Öppna en pull request mot aktuell default branch och beskriv vad som ändras och varför.
6. Läs hela review-state, inklusive kommentarer och trådar som GitHub markerar som `outdated`, och verifiera att grundproblemen är lösta i aktuell kod.
7. Låt automatiska kontroller och repository-regler bli gröna innan merge.
8. Färdigställ den öppna arbetsgrenen, draften eller PR:n innan agenten påbörjar nästa uppgift i samma repository. Om arbetet inte kan slutföras ska det uttryckligen avslutas eller markeras med konkret blocker.

Arbetet ska vara seriellt och semantiskt per repository. En arbetsgren/PR motsvarar en sammanhängande feature eller uppgift och `feature`-delen ska beskriva arbetet semantiskt.

Om `{agent}/{feature}/{date}` redan finns för uppgiften ska agenten fortsätta den arbetslinjen. Skapa inte tids-/ID-suffix eller parallella branchvarianter för att kringgå ett upptaget namn.

Commits ska använda Conventional Commits eller motsvarande tydlig typ, till exempel `feat:`, `fix:`, `docs:`, `chore:`, `ci:`, `build:`, `refactor:` eller `test:`.

## Pull requests

En bra pull request ska:

- lösa ett tydligt avgränsat problem,
- undvika orelaterade ändringar,
- behålla befintligt beteende om ändringen inte uttryckligen ska ändra det,
- innehålla eller uppdatera tester när det är relevant,
- uppdatera dokumentation när användarbeteende, konfiguration eller gränssnitt ändras,
- inte innehålla genererade filer, binärer eller andra artefakter som repositoryt inte redan versionshanterar.

## Kod och kvalitet

Följ repositoryts befintliga struktur, namngivning, formattering och verktyg. Om projektet har egna instruktioner i exempelvis `README.md`, `docs/`, `AGENTS.md` eller motsvarande gäller de före den här generella guiden.

Undvik onödiga beroenden och håll säkerhetsrelaterade förändringar så transparenta och granskningsbara som möjligt.

## Känslig information

Lägg aldrig in lösenord, tokens, API-nycklar, privata nycklar, personuppgifter eller andra hemligheter i commits, issues, pull requests, discussions, loggar eller skärmbilder.

Om känslig information råkar publiceras ska den behandlas som komprometterad och roteras eller återkallas där det är möjligt.

## Licens och rättigheter

Genom att bidra intygar du att du har rätt att skicka in ändringen. Bidrag omfattas av repositoryts licens och övriga tillämpliga villkor.

## Repository-specifika regler

Följ repositoryts egna `CONTRIBUTING.md`, `AGENTS.md` och dokumentation när sådana finns. Intern drift- och arkitekturkontext publiceras inte genom det här repositoryt.

Om ett repository innehåller en lokal `CONTRIBUTING.md` gäller den före den här gemensamma standarden.
