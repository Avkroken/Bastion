# bastion — AI Agent Guide

Fri, öppen, fristående SSH-klient. Varje plattform skrivs native i sitt eget
språk/UI-ramverk — inget delat cross-platform UI-lager (beslut 2026-07-29,
efter att SwiftCrossUI/WinUIBackend visade sig sakna funktioner Windows
faktiskt har). Sammankoppling mellan klienter sker via ett synkprotokoll,
inte via delad kod:

- `App/` (iOS/macOS, SwiftUI): native, bygger på `Sources/SSHCore` (ren
  SwiftNIO) — Swift ÄR native på Apple-plattformar, så delad kärna gäller
  bara här.
- `Android/` (Kotlin/Gradle): helt separat portering, Apache MINA SSHD
  istället för `SSHCore` — var redan förebilden för principen.
- `WindowsApp/` (C#/.NET + WinUI 3 + SSH.NET): under uppbyggnad från grunden,
  ersätter det tidigare SwiftCrossUI/WinUIBackend-spåret (borttaget).
- `LinuxApp/` (Rust + GTK4/gtk4-rs + russh/libssh2): under uppbyggnad från
  grunden, ersätter det tidigare SwiftCrossUI/GtkBackend-spåret (borttaget).

## Conventions

- Ny funktionalitet i kärnan (`SSHCore`) ska ha tester i `Tests/SSHCoreTests`
- `App/` byggs bara i Xcode — kan inte verifieras via `swift build` på Linux;
  CI:t (`.github/workflows/xcode.yml`) bygger det på en macOS-runner
- `Android/` byggs via `./gradlew` (kräver JDK 17+ och Android SDK
  command-line tools, se `Android/local.properties` som inte committas)
- `WindowsApp/` byggs via `dotnet build` (WinUI 3, kräver Windows App SDK)
- `LinuxApp/` byggs via `cargo build` (kräver `libgtk-4-dev` + `libadwaita-1-dev` +
  `libvte-2.91-gtk4-dev`)
- OAuth är PKCE-baserat — inga klienthemligheter i koden, bara publika klient-ID:n

## Allowed
- Committa på dev
- Skapa arbetsgrenar för PR:er (`claude/*`)
- Modify code
- Run tests
- Open PRs

## Forbidden
- Push directly to main/master
- Merge PRs på eget initiativ (be uttryckligen så är det okej)
- Ta bort grenar
- Disable workflows
- Modify secrets
- Change GitHub org settings

## Requirements
- All tests must pass (`swift test` i repo-roten)
- Keep PRs focused
- Never include unrelated changes
- Never commit credentials
- Never force push

## Svarsformat

Regeluppsättningen kommer från plugin:et `i-have-adhd`. Den laddas inte i
alla sessioner (t.ex. inte i Claude Code på webben), så den står här —
det här är källan som gäller oavsett var agenten kör.

Form:

- Led med åtgärden eller kommandot, inte med bakgrunden
- Numrera flerstegsprocesser, ett avgränsat steg per rad
- Max fem punkter per lista
- Hoppa över inledningar, sammanfattningar och avslutningsfraser
- Långa förklaringar bara på begäran

Innehåll:

- Säg uttryckligen vad som är gjort och vad som återstår
- Ange konkreta tidsuppskattningar
- Visa vad som fungerar efter en ändring, inte bara att den är gjord
- Vid fel: var, varför och hur det åtgärdas — kortfattat
- Avsluta med ett nästa steg som tar under två minuter
