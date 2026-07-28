# Installerar Bastion för Windows: kontrollerar/installerar det Windows App
# Runtime-beroende bastion-gui.exe kräver för att starta (annars "This
# application requires the Windows App Runtime Version 1.5" / krasch i
# swift-winuis SwiftApplication.main(), verifierat på riktig Windows Server
# 2025-hårdvara 2026-07-28) — INNAN bastion-gui.exe körs, inte något
# slutanvändaren ska behöva jaga upp själv. Se README.md i denna katalog.
#
# Körs som: .\Install-Bastion.ps1 [-BastionExePath <sökväg till bastion-gui.exe>]
param(
    [string]$BastionExePath = "$PSScriptRoot\.build\release\bastion-gui.exe"
)

$ErrorActionPreference = "Stop"

# Exakt version dokumenterad av swift-winui (github.com/moreSwift/swift-winui
# README, "Runtime dependencies") — swift-winui-bindningarna är kompilerade
# mot denna SPECIFIKA Windows App SDK-version, en senare 1.5.x-patch (t.ex.
# 1.5.250108004) räcker INTE (samma "requires Windows App Runtime Version
# 1.5"-fel kvarstod med en nyare patch i verifieringen 2026-07-28).
$RuntimeInstallerUrl = "https://aka.ms/windowsappsdk/1.5/1.5.240205001-preview1/windowsappruntimeinstall-x64.exe"

function Test-RuntimeInstalled {
    $pkg = Get-AppxPackage -Name "Microsoft.WindowsAppRuntime.1.5-preview1*" -ErrorAction SilentlyContinue
    return $null -ne $pkg
}

if (Test-RuntimeInstalled) {
    Write-Host "Windows App Runtime 1.5-preview1 redan installerat."
} else {
    Write-Host "Installerar Windows App Runtime 1.5-preview1 (krävs av bastion-gui)..."
    $installerPath = Join-Path $env:TEMP "windowsappruntimeinstall-x64.exe"
    Invoke-WebRequest -Uri $RuntimeInstallerUrl -OutFile $installerPath -UseBasicParsing
    # Måste köras i en RIKTIG interaktiv session (verifierat 2026-07-28) —
    # MSIX-paketregistrering nekar åtkomst (0x80070005) över en icke-
    # interaktiv fjärrshell (WinRM/PsExec utan -i). Fungerar normalt för
    # en användare som dubbelklickar det här scriptet eller kör det i en
    # egen PowerShell-terminal.
    $p = Start-Process -FilePath $installerPath -ArgumentList "--quiet" -Wait -PassThru
    if ($p.ExitCode -ne 0) {
        Write-Error "Windows App Runtime-installationen misslyckades (kod $($p.ExitCode)). Kör om detta script i en INTERAKTIV PowerShell-session (inte via fjärrskript/WinRM/CI) — MSIX-paketregistrering kräver en riktig inloggad session."
        exit 1
    }
    Write-Host "Windows App Runtime installerat."
}

if (Test-Path $BastionExePath) {
    Write-Host "Startar Bastion..."
    Start-Process -FilePath $BastionExePath
} else {
    Write-Host "Hittade ingen bastion-gui.exe på $BastionExePath — bygg klart först (swift build --product bastion-gui -c release)."
}
