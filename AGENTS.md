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

Arbete sker i en **sluten pool av tre grenar**, en per arbetstyp:

| Slot | För |
| --- | --- |
| `work/feature` | ny funktionalitet |
| `work/fix` | buggfixar och CI-problem |
| `work/chore` | dokumentation, städning, konfiguration |

`main` tar bara emot squash-mergade PR:er som passerat gröna checkar.

**Skapa aldrig egna grenar.** Rulesetet blockerar det — en push som försöker
skapa något utanför poolen avvisas. Poolen finns för att grenar som skapas per
uppgift blir liggande halvfärdiga.

1. Välj sloten som matchar arbetet. Är den upptagen duger vilken ledig som helst —
   namnen är vägledning, inte en spärr. Ligger det omergat arbete i en slot,
   **slutför det först** i stället för att börja något nytt i en annan.
2. Implementera och kör relevanta lokala tester innan push. Håll varje PR till en sammanhängande uppgift.
3. Pusha till sloten och öppna PR från den till `main` som klar för granskning.
   Aktivera auto-merge — merge-kön tar PR:n så snart required checks är gröna.
4. Lös CI- och reviewproblem i samma slot; PR:n uppdateras av varje push.
5. **Squash merge är den enda tillåtna merge-metoden.** Efter merge rebasar
   `.github/workflows/sync-pool.yml` varje slot på `main`.

Skicka aldrig direkt till `main`, kringgå inte branch protection/rulesets och ändra
inte hemligheter eller organisationsinställningar utan uttrycklig instruktion.

## Svarsformat

**[SKILLS.md](SKILLS.md) styr allt svarsformat. Läs den och följ den i varje svar.**

SKILLS.md har företräde framför den här filen och framför varje annan
formuleringsanvisning i repot. Sammanfatta den inte, återge den inte i kortform
och väg den inte mot andra skrivelser — det är den filen som gäller.
