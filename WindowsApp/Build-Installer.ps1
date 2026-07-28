# Bygger den kompletta BastionSetup.exe: hämtar Windows App Runtime-
# installeraren (buntas in i paketet, se Installer.iss) och kör Inno Setup.
# Kräver: swift build redan körd (.build\release\bastion-gui.exe finns),
# Inno Setup 6 installerat (iscc.exe i PATH eller default-sökväg).
$ErrorActionPreference = "Stop"

$RuntimeInstallerUrl = "https://aka.ms/windowsappsdk/1.5/1.5.240205001-preview1/windowsappruntimeinstall-x64.exe"
$RuntimeInstallerFile = Join-Path $PSScriptRoot "windowsappruntimeinstall-x64.exe"

if (-not (Test-Path $RuntimeInstallerFile)) {
    Write-Host "Hämtar Windows App Runtime-installeraren för buntning..."
    Invoke-WebRequest -Uri $RuntimeInstallerUrl -OutFile $RuntimeInstallerFile -UseBasicParsing
}

$exePath = Join-Path $PSScriptRoot ".build\release\bastion-gui.exe"
if (-not (Test-Path $exePath)) {
    Write-Error "Hittade ingen bastion-gui.exe på $exePath — kör 'swift build --product bastion-gui -c release' först."
    exit 1
}

$iscc = Get-Command "iscc.exe" -ErrorAction SilentlyContinue
if (-not $iscc) {
    $defaultPath = "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe"
    if (Test-Path $defaultPath) {
        $iscc = $defaultPath
    } else {
        Write-Error "Hittar inte iscc.exe. Installera Inno Setup 6 (https://jrsoftware.org/isdl.php) eller lägg till det i PATH."
        exit 1
    }
} else {
    $iscc = $iscc.Source
}

& $iscc (Join-Path $PSScriptRoot "Installer.iss")
Write-Host "Klar: WindowsApp\Output\BastionSetup.exe"
