; Inno Setup-skript: bygger EN installer-.exe som buntar bastion-gui.exe +
; Windows App Runtime 1.5-preview1 (körs tyst som en förinstallationsåtgärd),
; så slutanvändaren aldrig behöver jaga paket/paketversioner själv eller se
; PowerShell — direkt svar på packningsklagomålet 2026-07-28 (se README.md).
;
; Bygg med: iscc Installer.iss  (kräver Inno Setup 6, https://jrsoftware.org/isdl.php)
; Output: Output\BastionSetup.exe

#define MyAppName "Bastion"
#define MyAppExeName "bastion-gui.exe"
#define MyAppPublisher "Bastion"
#define RuntimeInstallerUrl "https://aka.ms/windowsappsdk/1.5/1.5.240205001-preview1/windowsappruntimeinstall-x64.exe"
#define RuntimeInstallerFile "windowsappruntimeinstall-x64.exe"

[Setup]
AppName={#MyAppName}
AppPublisher={#MyAppPublisher}
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
OutputDir=Output
OutputBaseFilename=BastionSetup
Compression=lzma2
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Runtime-installeraren registrerar ett MSIX-paket, vilket kräver en riktig
; inloggad session (0x80070005 över icke-interaktiva kontext) — samma skäl
; som Install-Bastion.ps1 kräver interaktiv körning, se README.md.
PrivilegesRequired=lowest

[Files]
Source: ".build\release\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
; Runtime-installeraren laddas ned vid byggtillfället (se Build-Installer.ps1)
; och buntas in här istället för att hämtas vid installation — så
; installationen fungerar även utan nätverksåtkomst på målmaskinen.
Source: "{#RuntimeInstallerFile}"; DestDir: "{tmp}"; Flags: deleteafterinstall

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"

[Run]
; Windows App Runtime 1.5-preview1 — exakt version bastion-gui.exe kräver
; (senare 1.5.x-patchar räcker inte, se README.md). Körs tyst, ignorerar
; ExitCode 0xB7 (already installed).
Filename: "{tmp}\{#RuntimeInstallerFile}"; Parameters: "--quiet"; \
    StatusMsg: "Installerar Windows App Runtime (krävs av Bastion)..."; \
    Flags: waituntilterminated
Filename: "{app}\{#MyAppExeName}"; Description: "Starta {#MyAppName}"; \
    Flags: nowait postinstall skipifsilent
