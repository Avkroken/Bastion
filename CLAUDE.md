# bastion — Claude Code Guide

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
- `LinuxApp/` byggs via `cargo build` (kräver GTK4-devpaket)
- OAuth är PKCE-baserat — inga klienthemligheter i koden, bara publika klient-ID:n
