#!/bin/sh

# Keep repository-level build discovery (for example CodeQL autobuild) on the
# actual Android project.  Gradle's generated wrapper uses the caller's current
# directory, so invoking Android/gradlew from the repository root otherwise
# makes Gradle look for settings in the wrong directory.
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)

exec "$SCRIPT_DIR/Android/gradlew" --project-dir "$SCRIPT_DIR/Android" "$@"
