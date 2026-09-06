# REPO.md

`Bastion` is a multi-platform SSH client with platform-specific build/test requirements.

## CI contract

Preserve the meaning of the live required checks when changing CI:

- `CI / android`: Gradle build/test.
- `CI / windows`: .NET tests and WinUI build.
- `CI / linux`: Rust/GTK build, tests and MSRV build.
- `CI / swift-linux`: Swift build/test in Linux.
- `CI / apple`: iOS/macOS/tvOS builds plus Swift package build/test.
- `scope-policy`: repository scope/branch validation where required by the live ruleset.

Packaging/TestFlight are release verification, not substitutes for merge-gate CI.

## Validation

Run the relevant platform build/tests for changed code. For CI changes, verify emitted check contexts against the live rulesets after pushing.

Pin third-party GitHub Actions to full commit SHAs. Do not rename/remove required checks without updating and verifying the live ruleset in the same migration.
