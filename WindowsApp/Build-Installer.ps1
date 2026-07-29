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

# Bunta Swift-runtime-DLL:erna (swiftCore.dll m.fl.) sa slutanvandaren inte
# behover ha Swift installerat -- utan detta kraschar bastion-gui.exe med
# "DLL saknas" pa en ren maskin. Hittar runtimen relativt swift.exe:s egen
# sokvag (...\Programs\Swift\Toolchains\<ver>\usr\bin\swift.exe -> leta efter
# syskonmappen ...\Programs\Swift\Runtimes\<ver>\usr\bin).
$swiftExe = (Get-Command "swift.exe" -ErrorAction SilentlyContinue).Source
if (-not $swiftExe) {
    Write-Error "Hittar inte swift.exe i PATH -- kan inte lokalisera Swift-runtime-DLL:erna att bunta."
    exit 1
}
$swiftProgramsRoot = Split-Path (Split-Path (Split-Path (Split-Path $swiftExe)))
$runtimeBinDir = Get-ChildItem (Join-Path $swiftProgramsRoot "Runtimes\*\usr\bin") -Directory -ErrorAction SilentlyContinue |
    Select-Object -First 1
if (-not $runtimeBinDir) {
    Write-Error "Hittar ingen Swift-runtime-katalog under $swiftProgramsRoot\Runtimes\*\usr\bin."
    exit 1
}
$redistDir = Join-Path $PSScriptRoot "Redist\SwiftRuntime"
New-Item -ItemType Directory -Path $redistDir -Force | Out-Null
Write-Host "Kopierar Swift-runtime-DLL:er fran $($runtimeBinDir.FullName)..."
Copy-Item (Join-Path $runtimeBinDir.FullName "*.dll") -Destination $redistDir -Force

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
