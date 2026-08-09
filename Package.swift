// swift-tools-version:5.9
import PackageDescription

// Bastion — kärnbibliotek + CLI för att bevisa SSH-transporten.
// SSHCore och bastion-cli bygger på Linux OCH Apple (ren SwiftNIO).
// Terminal-UI:t (SwiftTerm) ligger i App/ och byggs bara i Xcode — se App/README.md.
// Linux-GUI:t (SwiftCrossUI/GTK4) ligger i LinuxApp/ som ett EGET paket — se
// LinuxApp/Package.swift för varför det medvetet inte ligger här.
let package = Package(
    name: "bastion",
    platforms: [
        .macOS(.v13), .iOS(.v16),
    ],
    products: [
        .library(name: "SSHCore", targets: ["SSHCore"]),
        .executable(name: "bastion-cli", targets: ["bastion-cli"]),
    ],
    dependencies: [
        .package(url: "https://github.com/apple/swift-nio-ssh.git", from: "0.14.0"),
        // Pinnad till 2.86.2 (senast med swift-tools-version:5.10 — 2.87.0+
        // gick till 6.0/6.1) för att kringgå apple/swift-nio#3647: swift-nios
        // EGNA Sendable/IPPROTO-kompileringsfel på Windows, som bara triggas
        // när dess källor kompileras i Swift 6-strict-concurrency-läge —
        // styrs av PAKETETS EGEN deklarerade tools-version, inte konsumentens.
        // Bekräftat: windows-gui.yml gick grönt första gången någonsin med
        // den här pinningen (74 misslyckade körningar innan, 0 lyckade).
        // Ta bort pinningen och gå tillbaka till `from:` när uppströms
        // löser #3647 på riktigt.
        .package(url: "https://github.com/apple/swift-nio.git", exact: "2.101.3"),
        .package(url: "https://github.com/apple/swift-crypto.git", from: "4.5.0"),
    ],
    targets: [
        .target(
            name: "SSHCore",
            dependencies: [
                .product(name: "NIOSSH", package: "swift-nio-ssh"),
                .product(name: "NIOCore", package: "swift-nio"),
                .product(name: "NIOPosix", package: "swift-nio"),
                .product(name: "Crypto", package: "swift-crypto"),
            ]
        ),
        .executableTarget(
            name: "bastion-cli",
            dependencies: ["SSHCore"]
        ),
        .testTarget(
            name: "SSHCoreTests",
            dependencies: [
                "SSHCore",
                .product(name: "NIOEmbedded", package: "swift-nio"),
            ]
        ),
    ]
)
