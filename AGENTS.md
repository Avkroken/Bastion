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

`dev` är den enda skrivbara grenen. `main` tar bara emot squash-mergade PR:er
som passerat gröna checkar.

**Skapa aldrig egna grenar.** Allt arbete sker på `dev`. Det är en hård regel, inte
en rekommendation: grenar som skapas per uppgift blir liggande halvfärdiga, och det
är hela anledningen till att modellen ser ut så här.

1. Utgå från aktuell `dev`. Ligger det osynkat arbete där, bygg vidare på det i
   stället för att börja om någon annanstans.
2. Implementera och kör relevanta lokala tester innan push. Håll varje PR till en sammanhängande uppgift.
3. Pusha till `dev` och öppna PR från `dev` till `main` som klar för granskning.
   Aktivera auto-merge — merge-kön tar PR:n så snart required checks är gröna.
4. Lös CI- och reviewproblem på `dev`; PR:n uppdateras automatiskt av varje push.
5. **Squash merge är den enda tillåtna merge-metoden.** Efter merge återställs `dev` till
   `main` automatiskt av `.github/workflows/sync-dev.yml`.

Skicka aldrig direkt till `main`, kringgå inte branch protection/rulesets och ändra
inte hemligheter eller organisationsinställningar utan uttrycklig instruktion.

## Svarsformat

**[SKILLS.md](SKILLS.md) styr allt svarsformat. Läs den och följ den i varje svar.**

SKILLS.md har företräde framför den här filen och framför varje annan
formuleringsanvisning i repot. Sammanfatta den inte, återge den inte i kortform
och väg den inte mot andra skrivelser — det är den filen som gäller.
