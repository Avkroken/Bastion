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
  `libvte-2.91-gtk4-dev` + `libgtksourceview-5-dev`)
- OAuth är PKCE-baserat — inga klienthemligheter i koden, bara publika klient-ID:n

## Arbetsflöde: exakt en uppgift åt gången

Repositoryt har exakt två arbetsgrenar: `dev` och `main`. Skapa aldrig en tredje gren, inte ens tillfälligt. Allt utvecklingsarbete görs på `dev` och går via ett ändringsförslag från `dev` till `main`.

En agent får ha exakt en aktiv koduppgift åt gången. Flera uppgifter är en kö, inte parallellt arbete. Nästa uppgift får inte påbörjas förrän den aktuella uppgiften är mergad eller uttryckligen blockerad av något agenten inte kan lösa själv.

Arbeta lokalt så långt det är praktiskt innan du pushar. Samla sammanhängande ändringar, testfixar och följdjusteringar i meningsfulla batcher i stället för att pusha varje liten edit och därmed starta om CI i onödan. När en PR redan kör CI får du fortsätta analysera, testa och förbättra samma uppgift lokalt. Push endast när du har en ny sammanhängande batch som faktiskt behöver valideras. CI-väntan är aldrig ett skäl att börja på nästa uppgift.

För varje uppgift:

1. Synka `dev` med `main`. Om `dev` redan innehåller ofärdigt arbete, slutför det först.
2. Implementera och testa den aktuella uppgiften lokalt på `dev`; samla ändringar i så stora sammanhängande batcher som är rimliga.
3. Commit och push till `dev`, skapa eller uppdatera exakt ett PR `dev` → `main`, och aktivera auto-merge.
4. Medan CI/review pågår: fortsätt endast lokalt med samma uppgift. Lös relevanta fel och kommentarer och pusha dem samlat, inte en i taget. Efter varje push som ändrar PR-headen, och särskilt efter den sista pushen, verifiera uttryckligen att auto-merge fortfarande är aktiverad; återaktivera den om head-ändringen slog av den.
5. När PR:n är mergad, synka `dev` till `main`. Först därefter får nästa uppgift börja.

Om uppgiften blockeras av en extern åtgärd som agenten faktiskt inte kan utföra, dokumentera den exakta blockeraren och stanna. Börja inte en annan koduppgift utan uttrycklig instruktion från användaren.

## Tillåtet
- Ändra kod på `dev`
- Köra lokala tester och analyser
- Öppna ändringsförslag endast från `dev` till `main`
- Rätta CI- och reviewproblem för den aktiva uppgiften tills PR:n kan mergas

## Förbjudet
- Skapa andra grenar än `dev` och `main`
- Arbeta parallellt på flera koduppgifter
- Börja nästa uppgift medan den aktuella PR:n fortfarande är öppen eller blockerad
- Skicka ändringar direkt till `main` eller `master`
- Radera grenar
- Stänga av arbetsflöden
- Ändra hemligheter
- Ändra inställningar för GitHub-organisationen
- Tvinga igenom en push eller kringgå branch protection/rulesets

## Krav
- Överlämna kodändringar endast på `dev`
- Alla relevanta tester måste godkännas (`swift test` i repo-roten där det är tillämpligt)
- Håll varje ändringsförslag avgränsat till en uppgift
- Arbeta lokalt så mycket som möjligt och undvik onödigt täta pushar som startar om CI
- Ta aldrig med orelaterade ändringar
- Överlämna aldrig inloggningsuppgifter eller andra hemligheter till versionshistoriken
- Skapa ändringsförslag som klara för granskning, aldrig som utkast
- Aktivera automatisk sammanfogning med en metod som tillåts av förrådets regler direkt efter att ändringsförslaget skapats
- Efter varje push som ändrar PR-headen: verifiera att automatisk sammanfogning fortfarande är aktiv och återaktivera den vid behov
- Automatisk sammanfogning får slutföras först när alla regelkrav och kontrollkörningar har godkänts
- Om CI, review eller auto-merge blockerar leveransen: lös blockeraren för den aktiva uppgiften innan annat kodarbete påbörjas
- Om automatisk sammanfogning inte kan aktiveras: rapportera det exakta felet
- Efter merge: synka `dev` till `main` innan nästa uppgift

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
