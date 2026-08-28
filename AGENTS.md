# bastion — AI Agent Guide

Fri, öppen SSH-klient med native implementation per plattform. Plattformarna delar synkprotokoll, inte ett gemensamt UI-lager.

## Struktur

- `App/` — iOS/macOS, SwiftUI, bygger på `Sources/SSHCore`.
- `Android/` — Kotlin/Gradle, Apache MINA SSHD.
- `WindowsApp/` — C#/.NET + WinUI 3 + SSH.NET.
- `LinuxApp/` — Rust + GTK4/gtk4-rs.
- `Sources/SSHCore` + `Tests/SSHCoreTests` — Swift-kärna och tester för Apple-klienterna.

## Konventioner

- Ny kärnfunktionalitet i `SSHCore` ska ha relevanta tester.
- Apple-appen verifieras i Xcode/macOS-CI; `swift build` på Linux ersätter inte det.
- Android byggs med Gradle/JDK 17+, Windows med `dotnet build`, Linux med Cargo och GTK-systemberoenden.
- OAuth använder PKCE. Klienthemligheter får inte hårdkodas.
- GitHub Actions ska pinnas till commit-SHA när praktiskt möjligt.

## GitHub-arbetsflöde

Arbete sker via tillfälliga arbetsgrenar och pull requests till `main`. Arbetsgrenar får använda repo- eller agentvalda namn som `claude/*`, `codex/*`, `feature/*`, `fix/*` eller motsvarande. De återanvändbara `work/feature`, `work/fix`, `work/chore` och `docs/content` får fortfarande användas men är inte obligatoriska.

Bastions permanenta utvecklingsgrenar `platform/*` och `core/swift` är uttryckligen undantagna från watchdoggen och ska bevaras som långlivade grenar.

1. Implementera och kör relevanta lokala tester innan push. Håll varje PR till en sammanhängande uppgift.
2. Pusha arbetsgrenen och öppna en ready PR till `main`.
3. **Aktivera auto-merge omedelbart efter att PR:n skapats**, även medan CI eller review fortfarande pågår.
4. Required CI-checkar och olösta review-trådar är merge-blockerare. Läs och utvärdera alltid alla review-kommentarer; relevanta fynd ska åtgärdas i samma PR innan tråden markeras resolved.
5. Efter varje ny commit ska både CI och review-status kontrolleras igen. När required CI är grönt och alla review-trådar är resolved ska den redan armerade auto-merge-funktionen/merge-kön föra PR:n till `main`. Om det inte sker, identifiera exakt kvarvarande blockerare. **Squash merge är den enda tillåtna merge-metoden.**

`.github/workflows/pr-watchdog.yml` bevakar alla lokala branches utom `main`, merge-köns `gh-readonly-queue/*` och de permanenta undantagen ovan. En branch med unika commits som har saknat öppen PR i mer än 60 minuter får en ready PR till `main` och squash auto-merge armeras. Exakt samma HEAD öppnas inte på nytt om den redan har behandlats i en stängd PR. Watchdoggen avgör inte om arbetet är önskvärt eller mergebart; CI, review och merge-gates gör det.

`.github/workflows/sync-pool.yml` får fortsatt synka Bastions uttryckliga återanvändbara slots och permanenta branchmodell. Watchdog-bevakade slots som `work/*` och `docs/content` får inte resetta oskyddat unikt arbete innan watchdoggen hunnit göra det synligt som PR. Godtyckliga agentgrenar utanför den explicita sync-poolen får aldrig resettas av sync-pool.

Skicka aldrig direkt till `main`, kringgå inte branch protection/rulesets, required checks, review resolution eller merge queue och ändra inte hemligheter eller organisationsinställningar utan uttrycklig instruktion.

## Svarsformat

**[SKILLS.md](SKILLS.md) styr allt svarsformat. Läs den och följ den i varje svar.**

SKILLS.md har företräde framför den här filen och framför varje annan formuleringsanvisning i repot. Sammanfatta den inte, återge den inte i kortform och väg den inte mot andra skrivelser — det är den filen som gäller.
