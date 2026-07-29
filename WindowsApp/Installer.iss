; Inno Setup-skript: bygger EN installer-.exe som buntar bastion-gui.exe +
; Windows App Runtime 1.5-preview1 (kors tyst som en forinstallationsatgard),
; sa slutanvandaren aldrig behover jaga paket/paketversioner sjalv eller se
; PowerShell -- direkt svar pa packningsklagomalet 2026-07-28 (se README.md).
;
; Bygg med: iscc Installer.iss  (kraver Inno Setup 6, https://jrsoftware.org/isdl.php)
; Output: Output\BastionSetup.exe

#define MyAppName "Bastion"
#define MyAppExeName "bastion-gui.exe"
#define MyAppPublisher "Bastion"
#define RuntimeInstallerUrl "https://aka.ms/windowsappsdk/1.5/1.5.240205001-preview1/windowsappruntimeinstall-x64.exe"
#define RuntimeInstallerFile "windowsappruntimeinstall-x64.exe"

[Setup]
AppName={#MyAppName}
AppVersion=1.0
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
; Runtime-installeraren registrerar ett MSIX-paket, vilket kraver en riktig
; inloggad session (0x80070005 over icke-interaktiva kontext) -- samma skal
; som Install-Bastion.ps1 kraver interaktiv korning, se README.md.
PrivilegesRequired=lowest

[Files]
Source: ".build\release\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
; Runtime-installeraren laddas ned vid byggtillfallet (se Build-Installer.ps1)
; och buntas in har istallet for att hamtas vid installation -- sa
; installationen fungerar aven utan natverksatkomst pa malmaskinen.
Source: "{#RuntimeInstallerFile}"; DestDir: "{tmp}"; Flags: deleteafterinstall

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"

[Run]
; Windows App Runtime 1.5-preview1 -- exakt version bastion-gui.exe kraver
; (senare 1.5.x-patchar racker inte, se README.md). Kors tyst, ignorerar
; ExitCode 0xB7 (already installed).
Filename: "{tmp}\{#RuntimeInstallerFile}"; Parameters: "--quiet"; \
    StatusMsg: "Installerar Windows App Runtime (kravs av Bastion)..."; \
    Flags: waituntilterminated
Filename: "{app}\{#MyAppExeName}"; Description: "Starta {#MyAppName}"; \
    Flags: nowait postinstall skipifsilent
