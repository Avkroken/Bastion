#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
PACKAGE_FILE="$SCRIPT_DIR/Package.swift"

SWIFTTERM_VERSION=$(sed -nE 's/.*exact: "([0-9]+\.[0-9]+\.[0-9]+)".*/\1/p' "$PACKAGE_FILE" | head -n 1)

if [ -z "$SWIFTTERM_VERSION" ]; then
  echo "error: could not read SwiftTerm exact version from $PACKAGE_FILE" >&2
  exit 1
fi

export SWIFTTERM_VERSION
cd "$SCRIPT_DIR"
exec xcodegen generate --spec project.dependencies.yml "$@"
