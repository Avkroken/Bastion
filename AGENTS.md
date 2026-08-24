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
3. Öppna PR från arbetsbranchen till `main` som klar för granskning. Auto-merge är tillåtet och får aktiveras när PR:n är redo; GitHub mergar först när alla ruleset-krav är uppfyllda.
4. Lös CI- och reviewproblem på samma arbetsbranch. Alla required checks och review-trådar ska vara klara innan merge.
5. **Squash merge är den enda tillåtna merge-metoden.** Använd inte merge commits eller rebase merge. Repot är konfigurerat att automatiskt radera den kortlivade head-branchen efter merge.

Skicka aldrig direkt till `main`, force-pusha inte förbi skydd och kringgå inte branch protection/rulesets. Ändra inte hemligheter eller organisationsinställningar utan uttrycklig instruktion.

## Svarsformat

- Led med nästa konkreta åtgärd eller resultat.
- Numrera flerstegsarbete och håll listor korta.
- Säg tydligt vad som är gjort och vad som återstår.
- Vid fel: ange var felet finns, orsaken och nästa fix.
