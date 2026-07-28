// swift-tools-version:5.10
import PackageDescription

// Eget paket, som LinuxApp/ — se den filens kommentar för varför GUI-paket
// hålls skilda från rotens Package.swift. Windows-motsvarigheten till
// LinuxApp/, via SwiftCrossUIs WinUIBackend istället för GtkBackend. Verifierat
// 2026-07-28 på en riktig Windows Server 2025-maskin (utöver CI:s
// windows-latest-runner): kräver Windows App Runtime 1.5-preview1 installerat
// separat innan bastion-gui.exe kan starta (annars "This application requires
// the Windows App Runtime Version 1.5" / krasch i swift-winuis
// SwiftApplication.main()) — se WindowsApp/Install-Bastion.ps1 och
// WindowsApp/README.md för den paketerade lösningen.
let package = Package(
    name: "bastion-gui",
    dependencies: [
        .package(path: ".."),
        .package(url: "https://github.com/moreSwift/swift-cross-ui.git", from: "0.8.0"),
    ],
    targets: [
        .executableTarget(
            name: "bastion-gui",
            dependencies: [
                .product(name: "SSHCore", package: "bastion"),
                .product(name: "SwiftCrossUI", package: "swift-cross-ui"),
                .product(name: "WinUIBackend", package: "swift-cross-ui"),
            ]
        )
    ]
)
