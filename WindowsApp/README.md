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

### Alternativ 1: `BastionSetup.exe` (rekommenderat för slutanvändare)

En enda installer-fil buntar `bastion-gui.exe` + Windows App Runtime
tillsammans — ingen PowerShell, ingen manuell paketjakt. Bygg den själv:

```powershell
.\Build-Installer.ps1
```

(kräver [Inno Setup 6](https://jrsoftware.org/isdl.php)). Scriptet hämtar
runtime-installeraren en gång, buntar in den i paketet enligt
`Installer.iss`, och skriver ut `Output\BastionSetup.exe`. Den filen är vad
som ska delas ut till slutanvändare — dubbelklick installerar allt (Bastion
+ runtime-beroendet) i ett svep.

Detta är det direkta svaret på packningsklagomålet 2026-07-28: "om bastion
ska installeras så måste allt som den behöver följas med... så folk inte
behöver jaga paket och paketversioner över hela internet."

### Alternativ 2: `Install-Bastion.ps1` (utvecklare/snabbtest)

```powershell
.\Install-Bastion.ps1
```

Kontrollerar om runtimen redan finns, installerar den annars (tyst) via
nätverksnedladdning, och startar sedan `bastion-gui.exe`. Måste köras i en
INTERAKTIV PowerShell-session (dubbelklick eller egen terminal) —
MSIX-paketregistrering nekar åtkomst (`0x80070005`) över en icke-interaktiv
fjärrshell som WinRM. Kräver nätverksåtkomst vid körning, till skillnad från
`BastionSetup.exe` som redan har runtime-installeraren inbäddad.

## Kända begränsningar

- Platshållar-UI just nu ("Bastion för Windows", 0 sparade värdar) — hela
  UI:t porteras hit i ett senare steg, se ROADMAP.md.
- `BastionSetup.exe` måste byggas manuellt (`Build-Installer.ps1`) tills
  detta görs till ett CI-artefakt-steg — den färdigbyggda filen distribueras
  inte i repot.
