# BASTION.md

Repository-specific instructions for `Avkroken/Bastion`. These instructions supplement the canonical Avkroken policy in `Avkroken/.github/AGENTS.md`.

## Repository-specific CI contract

Bastion's required platform workflows are direct platform verifications rather than routing layers. Preserve the meaning of each check when changing CI:

- Android: Gradle build/test.
- Windows: .NET core tests and WinUI build.
- Linux: Rust/GTK build, tests and MSRV build.
- Swift Linux: Swift build/test in the Linux container.
- Apple: iOS/macOS/tvOS builds plus Swift package build/test.
- `scope-policy`: limits only the explicitly named `platform/*` and `core/swift` branches.
- OSV: dependency scanning according to the repository's current workflow/ruleset configuration.

Packaging and TestFlight workflows are product/release verification and are separate from merge-gate CI.

GitHub Actions references must remain pinned to full commit SHAs.

## Local validation

For platform changes, run the relevant platform build/test commands defined by the repository. For CI changes, verify the emitted GitHub check contexts against the live rulesets after pushing.

## Response format

Read and follow `SKILLS.md` when working in this repository.
