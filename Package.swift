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
        // Exakt pinnad p.g.a. apple/swift-nio#3647: swift-nios EGNA
        // Sendable/IPPROTO-kompileringsfel på Windows, som bara triggas när
        // dess källor kompileras i Swift 6-strict-concurrency-läge — styrs av
        // PAKETETS EGEN deklarerade tools-version, inte konsumentens.
        // Pinningen infördes ursprungligen på 2.86.2 (windows-gui.yml gick då
        // grönt för första gången: 74 misslyckade körningar innan, 0 lyckade)
        // och har sedan flyttats upp till 2.101.3, som i skrivande stund är
        // senaste releasen — pinningen håller alltså INTE tillbaka versionen.
        // Den finns kvar enbart för att hindra automatiska bumpar innan
        // #3647 är löst (Renovate-bump i PR #153 återinförde exakt den bugg
        // PR #149 fixade). Kontrollerat 2026-08-11: #3647 fortfarande öppen.
        // Byt tillbaka till `from:` när uppströms löser #3647 på riktigt.
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
