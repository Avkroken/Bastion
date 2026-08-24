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

`main` är den enda långlivade arbetsgrenen. `dev` används inte.

1. Börja varje uppgift från aktuell `main` på en ny kortlivad branch, till exempel `fix/...`, `feat/...` eller `chore/...`.
2. Implementera och kör relevanta lokala tester innan push. Håll branchen och PR:n till en sammanhängande uppgift.
3. Öppna PR från arbetsbranchen till `main` som klar för granskning. Aktivera inte auto-merge.
4. Lös CI- och reviewproblem på samma arbetsbranch. Alla required checks och review-trådar ska vara klara innan merge.
5. Merge sker med **squash merge**. Använd inte merge commits eller rebase merge. Den kortlivade head-branchen får raderas efter merge.

Skicka aldrig direkt till `main`, force-pusha inte förbi skydd och kringgå inte branch protection/rulesets. Ändra inte hemligheter eller organisationsinställningar utan uttrycklig instruktion.

## Svarsformat

**[SKILLS.md](SKILLS.md) styr allt svarsformat. Läs den och följ den i varje svar.**

SKILLS.md har företräde framför den här filen och framför varje annan
formuleringsanvisning i repot. Sammanfatta den inte, återge den inte i kortform
och väg den inte mot andra skrivelser — det är den filen som gäller.
