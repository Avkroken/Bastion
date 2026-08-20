#!/usr/bin/env bash
set -euo pipefail

script="$(cd "$(dirname "$0")" && pwd)/ci-impact.sh"

check_case() {
  local name="$1"
  local files="$2"
  shift 2

  local output
  output="$(mktemp)"
  CI_CHANGED_FILES="$files" GITHUB_OUTPUT="$output" bash "$script" >/dev/null

  local expected key actual
  for expected in "$@"; do
    key="${expected%%=*}"
    expected="${expected#*=}"
    actual="$(awk -F= -v k="$key" '$1 == k { value=$2 } END { print value }' "$output")"
    if [[ "$actual" != "$expected" ]]; then
      echo "FAIL $name: $key expected=$expected actual=$actual" >&2
      cat "$output" >&2
      rm -f "$output"
      exit 1
    fi
  done

  rm -f "$output"
  echo "PASS $name"
}

check_case docs 'AGENTS.md' \
  all=false apple_app=false swift_core=false android=false windows=false linuxapp=false cli_package=false

check_case apple 'App/ContentView.swift' \
  all=false apple_app=true swift_core=false android=false windows=false linuxapp=false cli_package=false

check_case sshcore 'Sources/SSHCore/SSHClient.swift' \
  all=false apple_app=true swift_core=true android=false windows=false linuxapp=false cli_package=true

check_case cli 'Sources/bastion-cli/main.swift' \
  all=false apple_app=false swift_core=true android=false windows=false linuxapp=false cli_package=true

check_case tests 'Tests/SSHCoreTests/SSHClientTests.swift' \
  all=false apple_app=false swift_core=true android=false windows=false linuxapp=false cli_package=false

check_case android 'Android/app/src/main/java/se/denied/bastion/MainActivity.kt' \
  all=false apple_app=false swift_core=false android=true windows=false linuxapp=false cli_package=false

check_case windows 'WindowsApp/MainWindow.xaml.cs' \
  all=false apple_app=false swift_core=false android=false windows=true linuxapp=false cli_package=false

check_case linuxapp 'LinuxApp/src/main.rs' \
  all=false apple_app=false swift_core=false android=false windows=false linuxapp=true cli_package=false

check_case protocol 'docs/protocol/sync-v1.md' \
  all=false apple_app=true swift_core=true android=true windows=true linuxapp=true cli_package=true

check_case unknown 'some-new-build-system.conf' \
  all=true apple_app=true swift_core=true android=true windows=true linuxapp=true cli_package=true

check_case mixed $'AGENTS.md\nWindowsApp/MainWindow.xaml' \
  all=false apple_app=false swift_core=false android=false windows=true linuxapp=false cli_package=false

echo 'All CI impact tests passed.'
