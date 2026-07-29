# Bygger den kompletta BastionSetup.exe: hamtar Windows App Runtime-
# installeraren (buntas in i paketet, se Installer.iss) och kor Inno Setup.
# Kraver: swift build redan kord (.build\release\bastion-gui.exe finns),
# Inno Setup 6 installerat (iscc.exe i PATH eller default-sokvag).
$ErrorActionPreference = "Stop"

$RuntimeInstallerUrl = "https://aka.ms/windowsappsdk/1.5/1.5.240205001-preview1/windowsappruntimeinstall-x64.exe"
$RuntimeInstallerFile = Join-Path $PSScriptRoot "windowsappruntimeinstall-x64.exe"

if (-not (Test-Path $RuntimeInstallerFile)) {
    Write-Host "Hamtar Windows App Runtime-installeraren for buntning..."
    Invoke-WebRequest -Uri $RuntimeInstallerUrl -OutFile $RuntimeInstallerFile -UseBasicParsing
}

$exePath = Join-Path $PSScriptRoot ".build\release\bastion-gui.exe"
if (-not (Test-Path $exePath)) {
    Write-Error "Hittade ingen bastion-gui.exe pa $exePath -- kor 'swift build --product bastion-gui -c release' forst."
    exit 1
}

$iscc = Get-Command "iscc.exe" -ErrorAction SilentlyContinue
if (-not $iscc) {
    $defaultPath = "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe"
    if (Test-Path $defaultPath) {
        $iscc = $defaultPath
    } else {
        Write-Error "Hittar inte iscc.exe. Installera Inno Setup 6 (https://jrsoftware.org/isdl.php) eller lagg till det i PATH."
        exit 1
    }
} else {
    $iscc = $iscc.Source
}

& $iscc (Join-Path $PSScriptRoot "Installer.iss")
Write-Host "Klar: WindowsApp\Output\BastionSetup.exe"
