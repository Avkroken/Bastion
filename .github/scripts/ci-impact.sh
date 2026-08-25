#!/usr/bin/env bash
# .github/scripts/ci-impact.sh
#
# Avgör vilka plattformsjobb som behöver köras för en given diff och skriver
# flaggorna till $GITHUB_OUTPUT.
#
# Anropas med CI_BASE_SHA och CI_HEAD_SHA satta av workflowen.
#
# Utdata (alla 'true'/'false'):
#   apple  android  windows  linux  cli  swift
#
# Principen är fail-open mot bredare CI: känner skriptet inte igen en sökväg
# antas den vara delad och allt körs. Endast rena dokumentationsändringar
# räknas som utan påverkan. Ett skript som gissar fel åt andra hållet släpper
# igenom otestad kod.

set -euo pipefail

: "${CI_BASE_SHA:?CI_BASE_SHA saknas}"
: "${CI_HEAD_SHA:?CI_HEAD_SHA saknas}"
OUT="${GITHUB_OUTPUT:-/dev/stdout}"

apple=false; android=false; windows=false; linux=false; cli=false; swift=false

emit() {
  {
    echo "apple=$apple"
    echo "android=$android"
    echo "windows=$windows"
    echo "linux=$linux"
    echo "cli=$cli"
    echo "swift=$swift"
  } >> "$OUT"
}

all_true() { apple=true; android=true; windows=true; linux=true; cli=true; swift=true; }

# Okänd bas (t.ex. första push till en ny gren) -> kör allt.
if [[ -z "$CI_BASE_SHA" || "$CI_BASE_SHA" =~ ^0+$ ]] \
   || ! git cat-file -e "${CI_BASE_SHA}^{commit}" 2>/dev/null; then
  echo "Okänd bas-SHA; kör alla plattformsjobb." >&2
  all_true; emit; exit 0
fi

changed="$(git diff --name-only --diff-filter=ACMRDTUXB "$CI_BASE_SHA" "$CI_HEAD_SHA")"
if [[ -z "$changed" ]]; then
  echo "Tom diff; inga plattformsjobb." >&2
  emit; exit 0
fi

while IFS= read -r f; do
  [[ -n "$f" ]] || continue
  case "$f" in
    App/*)                      apple=true ;;
    Android/*)                  android=true ;;
    WindowsApp/*)               windows=true ;;
    LinuxApp/*)                 linux=true ;;
    Sources/bastion-cli/*)      cli=true ;;
    # Swift-kärnan är delad av Apple-, CLI- och Linux-byggena.
    Sources/SSHCore/*|Tests/SSHCoreTests/*|Package.swift|Package.resolved)
                                swift=true; apple=true; cli=true; linux=true ;;
    # Ren dokumentation påverkar inga byggen.
    docs/*|*.md|*.txt|LICENSE|LICENSE.*) ;;
    # Allt annat (CI, verktyg, rotkonfiguration, okända sökvägar) är delat.
    *)
      echo "Delad/okänd sökväg '$f'; kör alla plattformsjobb." >&2
      all_true
      break
      ;;
  esac
done <<< "$changed"

emit
