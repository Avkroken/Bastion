#!/usr/bin/env bash
set -euo pipefail

# Bastion CI impact detector.
#
# Goal: route expensive platform CI from the actual diff instead of running
# every platform for every change. The detector is deliberately conservative:
# anything it cannot classify safely expands to all build targets.
#
# Outputs are written to $GITHUB_OUTPUT when available, otherwise stdout.
# Expected outputs:
#   apple_app, swift_core, android, windows, linuxapp, cli_package, all

out_file="${GITHUB_OUTPUT:-/dev/stdout}"

emit() {
  printf '%s=%s\n' "$1" "$2" >> "$out_file"
}

all=false
apple_app=false
swift_core=false
android=false
windows=false
linuxapp=false
cli_package=false

# Manual runs are verification runs: never silently skip anything.
if [[ "${GITHUB_EVENT_NAME:-}" == "workflow_dispatch" ]]; then
  all=true
fi

base="${CI_BASE_SHA:-}"
head="${CI_HEAD_SHA:-${GITHUB_SHA:-HEAD}}"

# A zero SHA is used for the first push to a new ref. There is no safe diff
# base in that case, so fail open and run everything.
if [[ -z "$base" || "$base" =~ ^0+$ ]]; then
  all=true
fi

changed=""
if [[ "$all" != true ]]; then
  if ! git cat-file -e "${base}^{commit}" 2>/dev/null || ! git cat-file -e "${head}^{commit}" 2>/dev/null; then
    all=true
  else
    changed="$(git diff --name-only --diff-filter=ACMRDTUXB "$base" "$head")"
  fi
fi

if [[ "$all" != true && -z "$changed" ]]; then
  # Empty/unknown diff should never be interpreted as permission to skip CI.
  all=true
fi

while IFS= read -r file; do
  [[ -n "$file" ]] || continue

  case "$file" in
    # Apple UI/project. This does not require the standalone Swift CLI/package.
    App/*)
      apple_app=true
      ;;

    # Shared Swift SSHCore is consumed by the Apple app and the root SwiftPM
    # package. CLI packages also embed it.
    Sources/SSHCore/*)
      apple_app=true
      swift_core=true
      cli_package=true
      ;;

    # The CLI is a root SwiftPM product but is not linked into the Apple app.
    Sources/bastion-cli/*)
      swift_core=true
      cli_package=true
      ;;

    # SSHCore tests exercise root SwiftPM only; rebuilding the Apple UI adds no
    # signal for a test-only change.
    Tests/SSHCoreTests/*)
      swift_core=true
      ;;

    Package.swift|Package.resolved)
      apple_app=true
      swift_core=true
      cli_package=true
      ;;

    Android/*)
      android=true
      ;;

    WindowsApp/*)
      windows=true
      ;;

    LinuxApp/*)
      linuxapp=true
      ;;

    # A protocol/spec change is intentionally cross-platform. These patterns
    # also reserve a stable place for future machine-readable protocol specs.
    docs/protocol/*|Protocol/*|PROTOCOL.md|SYNC_PROTOCOL.md)
      apple_app=true
      swift_core=true
      android=true
      windows=true
      linuxapp=true
      cli_package=true
      ;;

    # Workflow-local changes run the workflow they modify.
    .github/workflows/xcode.yml)
      apple_app=true
      swift_core=true
      ;;
    .github/workflows/swiftpm-linux.yml)
      swift_core=true
      ;;
    .github/workflows/android-build.yml)
      android=true
      ;;
    .github/workflows/windowsapp-build.yml)
      windows=true
      ;;
    .github/workflows/linuxapp-build.yml|.github/workflows/linuxapp-packaging.yml|.github/workflows/linuxapp-packaging-rpm.yml)
      linuxapp=true
      ;;
    .github/workflows/linux-packaging.yml|.github/workflows/linux-packaging-rpm.yml)
      cli_package=true
      ;;

    # The impact detector itself is security-critical routing logic. Any change
    # to it proves the full matrix before it is trusted.
    .github/scripts/ci-impact.sh)
      all=true
      ;;

    # Documentation and agent/process metadata do not affect binaries.
    *.md|LICENSE|LICENSE.*|docs/*|.github/dependabot.yml|.github/renovate.json|renovate.json)
      ;;

    # Unknown CI/action/config changes can affect arbitrary platforms. Fail
    # open rather than risk a false negative.
    .github/*|scripts/*|Makefile|*.yml|*.yaml|*.json|*.toml|*.lock)
      all=true
      ;;

    # Unknown source/config files are also conservative by default.
    *)
      all=true
      ;;
  esac
done <<< "$changed"

if [[ "$all" == true ]]; then
  apple_app=true
  swift_core=true
  android=true
  windows=true
  linuxapp=true
  cli_package=true
fi

emit all "$all"
emit apple_app "$apple_app"
emit swift_core "$swift_core"
emit android "$android"
emit windows "$windows"
emit linuxapp "$linuxapp"
emit cli_package "$cli_package"

printf 'CI impact: all=%s apple_app=%s swift_core=%s android=%s windows=%s linuxapp=%s cli_package=%s\n' \
  "$all" "$apple_app" "$swift_core" "$android" "$windows" "$linuxapp" "$cli_package"
if [[ -n "$changed" ]]; then
  printf 'Changed files:\n%s\n' "$changed"
fi
