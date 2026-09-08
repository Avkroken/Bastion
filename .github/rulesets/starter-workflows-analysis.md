# Starter-workflows analysis

This repository may only use workflow/configuration templates from `actions/starter-workflows`.

## State before the reset

The previous repository ruleset required `CI / android`, `CI / windows`, `CI / linux`, `CI / swift-linux`, `CI / apple`, and `scope-policy`. Those names came from repository-specific orchestration and cannot remain required after that orchestration is removed.

The current Actions history also shows GitHub dynamic/default CodeQL checks for several languages (`Analyze (csharp)`, `Analyze (java-kotlin)`, `Analyze (rust)`, `Analyze (ruby)`, `Analyze (swift)`, and `Analyze (actions)`). Their language-specific and dynamic nature makes them unsuitable as a single universal organization-level required status check.

## Selected starter templates

- `ci/swift.yml`: fits the root `Package.swift` directly and keeps the starter build/test structure unchanged apart from filling `main` and pinning the starter action major version to its exact commit.
- `code-scanning/dependency-review.yml`: applies to dependency-manifest changes across the repository.
- `.github/dependabot.yml`: retained in the starter configuration structure and filled with the repository's actual ecosystems: Swift, Bundler, Cargo, Gradle, NuGet, and GitHub Actions, all weekly.

## Documented gaps instead of custom workflows

Android, Windows, Rust/Linux, Apple/Xcode, packaging, TestFlight, release orchestration, OSV orchestration, and the former scope-policy checks are not rebuilt as custom workflows.

GitHub does publish generic starter templates for Android, .NET, Rust, and Xcode/iOS. In this repository those templates do not directly match the existing project layout and platform-specific commands without repository-specific working-directory, platform, SDK, or packaging logic. That behavior is therefore documented rather than recreated beyond the standard templates' frames.

The previous Windows build, for example, required a Windows runner and `dotnet build WindowsApp/WindowsApp.csproj -c Release -p:Platform=x64`, while GitHub's generic `.NET` starter runs on Ubuntu and assumes root-level `dotnet restore`, `build`, and `test`. The previous aggregator check `CI / windows` itself only asserted success of that separate platform job.

## Ruleset evidence

The selected starter workflows have now completed successfully on the reset branch and produced these exact job/check names:

- `Swift` -> `build`
- `Dependency review` -> `dependency-review`

The repository-specific ruleset file therefore requires only `build` and `dependency-review`. No former repository-specific check names are carried forward and no check name is inferred from a template without an observed successful run.
