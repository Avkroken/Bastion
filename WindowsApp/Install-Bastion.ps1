# Installerar Bastion for Windows: kontrollerar/installerar det Windows App
# Runtime-beroende bastion-gui.exe kraver for att starta (annars "This
# application requires the Windows App Runtime Version 1.5" / krasch i
# swift-winuis SwiftApplication.main(), verifierat pa riktig Windows Server
# 2025-hardvara 2026-07-28) -- INNAN bastion-gui.exe kors, inte nagot
# slutanvandaren ska behova jaga upp sjalv. Se README.md i denna katalog.
#
# Kors som: .\Install-Bastion.ps1 [-BastionExePath <sokvag till bastion-gui.exe>]
param(
    [string]$BastionExePath = "$PSScriptRoot\.build\release\bastion-gui.exe"
)

$ErrorActionPreference = "Stop"

# Exakt version dokumenterad av swift-winui (github.com/moreSwift/swift-winui
# README, "Runtime dependencies") -- swift-winui-bindningarna ar kompilerade
# mot denna SPECIFIKA Windows App SDK-version, en senare 1.5.x-patch (t.ex.
# 1.5.250108004) racker INTE (samma "requires Windows App Runtime Version
# 1.5"-fel kvarstod med en nyare patch i verifieringen 2026-07-28).
$RuntimeInstallerUrl = "https://aka.ms/windowsappsdk/1.5/1.5.240205001-preview1/windowsappruntimeinstall-x64.exe"

function Test-RuntimeInstalled {
    $pkg = Get-AppxPackage -Name "Microsoft.WindowsAppRuntime.1.5-preview1*" -ErrorAction SilentlyContinue
    return $null -ne $pkg
}

if (Test-RuntimeInstalled) {
    Write-Host "Windows App Runtime 1.5-preview1 redan installerat."
} else {
    Write-Host "Installerar Windows App Runtime 1.5-preview1 (kravs av bastion-gui)..."
    $installerPath = Join-Path $env:TEMP "windowsappruntimeinstall-x64.exe"
    Invoke-WebRequest -Uri $RuntimeInstallerUrl -OutFile $installerPath -UseBasicParsing
    # Maste koras i en RIKTIG interaktiv session (verifierat 2026-07-28) --
    # MSIX-paketregistrering nekar atkomst (0x80070005) over en icke-
    # interaktiv fjarrshell (WinRM/PsExec utan -i). Fungerar normalt for
    # en anvandare som dubbelklickar det har scriptet eller kor det i en
    # egen PowerShell-terminal.
    $p = Start-Process -FilePath $installerPath -ArgumentList "--quiet" -Wait -PassThru
    if ($p.ExitCode -ne 0) {
        Write-Error "Windows App Runtime-installationen misslyckades (kod $($p.ExitCode)). Kor om detta script i en INTERAKTIV PowerShell-session (inte via fjarrskript/WinRM/CI) -- MSIX-paketregistrering kraver en riktig inloggad session."
        exit 1
    }
    Write-Host "Windows App Runtime installerat."
}

if (Test-Path $BastionExePath) {
    Write-Host "Startar Bastion..."
    Start-Process -FilePath $BastionExePath
} else {
    Write-Host "Hittade ingen bastion-gui.exe pa $BastionExePath -- bygg klart forst (swift build --product bastion-gui -c release)."
}
