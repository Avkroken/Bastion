// swift-tools-version:5.9
import PackageDescription

// Dependency manifest for Apple-app-only Swift packages.
//
// XcodeGen continues to generate the application project from project.yml.
// This manifest makes SwiftTerm visible to Dependabot's SwiftPM updater.
let package = Package(
    name: "BastionAppDependencies",
    dependencies: [
        .package(
            url: "https://github.com/migueldeicaza/SwiftTerm.git",
            exact: "1.19.0"
        ),
    ]
)
