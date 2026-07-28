# Bastion för Windows

Windows-motsvarigheten till `LinuxApp/` — SwiftCrossUI med `WinUIBackend`
istället för `GtkBackend`. Verifierat på riktig Windows Server 2025-hårdvara
2026-07-28 (utöver CI:s `windows-latest`-runner i
`.github/workflows/windows-gui.yml`).

## Bygga

```powershell
swift build --product bastion-gui -c release -Xcc -D_ALLOW_COMPILER_AND_STL_VERSION_MISMATCH
```

Kräver Swift 6.1 (`swift-6.1-release`) och Visual Studio Build Tools med
C++-arbetsbelastningen (MSVC-linkern) — se `.github/workflows/windows-gui.yml`
för exakt uppsättning.

## Köra / installera

**`bastion-gui.exe` startar INTE på egen hand** — den kräver Windows App
Runtime 1.5-preview1 installerat separat (swift-winui-bindningarna är
kompilerade mot den specifika versionen; en senare 1.5.x-patch räcker inte,
och `.exe`-filen bär inte med sig detta beroende). Utan det: krasch
("This application requires the Windows App Runtime Version 1.5" eller
`WinUI/SwiftApplication.swift:64: Fatal error: fatal` i swift-winui).

Kör därför:

```powershell
.\Install-Bastion.ps1
```

Scriptet kontrollerar om runtimen redan finns, installerar den annars (tyst),
och startar sedan `bastion-gui.exe`. Måste köras i en INTERAKTIV
PowerShell-session (dubbelklick eller egen terminal) — MSIX-paketregistrering
nekar åtkomst (`0x80070005`) över en icke-interaktiv fjärrshell som WinRM.

Detta script är den paketerade lösningen på att sluta be slutanvändare jaga
rätt paket/paketversioner på egen hand (TestFlight/hårdvaru-feedback
2026-07-28) — allt `bastion-gui` behöver ska följa med installationsflödet.

## Kända begränsningar

- Platshållar-UI just nu ("Bastion för Windows", 0 sparade värdar) — hela
  UI:t porteras hit i ett senare steg, se ROADMAP.md.
- Ingen riktig installer (MSI/WiX/Inno Setup) än — `Install-Bastion.ps1` är
  ett förstahandssteg, inte slutlösningen. En riktig installer bör bunta
  runtime-installeraren och köra den som en förinstallationsåtgärd, så
  användaren aldrig ser PowerShell över huvud taget.
