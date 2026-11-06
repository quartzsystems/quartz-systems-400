#!/usr/bin/env bash
# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# VERSION, at the root, is QS/400's version (MAJOR.MINOR.PATCH) and
# everything else follows it: the release VxRyMz (lic/release reads VERSION
# at build time, so every panel, the media, the release directory and the
# ISO's name), and the GitHub release. The crates keep version 0.1.0: they
# are not published. docs/releasing.md.
#
#   ci/version.sh               print the version, 1.1.0
#   ci/version.sh release       print the release, V1R1M0
#   ci/version.sh check [TAG]   fail unless lic/release's KNOWN lists the
#                               release; with a tag, also unless the tag is
#                               v<VERSION> and CHANGELOG.md has its section
#   ci/version.sh notes         print CHANGELOG.md's section for VERSION
#   ci/version.sh prepare X.Y.Z write X.Y.Z to VERSION and turn CHANGELOG.md's
#                               Unreleased section into X.Y.Z's, dated today
set -euo pipefail
cd "$(dirname "$0")/.."

semver='^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'

version() {
  local v
  v=$(tr -d ' \r\n' < VERSION)
  [[ $v =~ $semver ]] || { echo "VERSION: '$v' is not MAJOR.MINOR.PATCH" >&2; exit 1; }
  echo "$v"
}

# CHANGELOG.md's section for $1: from its "## $1" heading to the next "## ".
notes() {
  tr -d '\r' < CHANGELOG.md | awk -v v="$1" '
    /^## / { if (on) exit; on = ($2 == v) ; next }
    on { print }
  ' | sed -e '/./,$!d' | sed -e ':a' -e '/^\n*$/{$d;N;ba' -e '}'
}

release() {
  IFS=. read -r major minor patch <<< "$(version)"
  echo "V${major}R${minor}M${patch}"
}

case "${1:-}" in
  "")
    version
    ;;
  release)
    release
    ;;
  check)
    v=$(version)
    IFS=. read -r major minor patch <<< "$v"
    grep -q "Release { v: $major, r: $minor, m: $patch }" lic/release/src/lib.rs \
      || { echo "lic/release/src/lib.rs KNOWN does not list $(release)" >&2; exit 1; }
    if [ -n "${2:-}" ]; then
      [ "$2" = "v$v" ] || { echo "tag $2 is not v$v, the version in VERSION" >&2; exit 1; }
      [ -n "$(notes "$v")" ] || { echo "CHANGELOG.md has no '## $v' section, or it is empty" >&2; exit 1; }
    fi
    echo "version $v ($(release))"
    ;;
  notes)
    notes "$(version)"
    ;;
  prepare)
    new=${2:-}
    [[ $new =~ $semver ]] || { echo "usage: ci/version.sh prepare MAJOR.MINOR.PATCH" >&2; exit 2; }
    if tr -d '\r' < CHANGELOG.md | grep -q "^## $new\( \|$\)"; then
      echo "CHANGELOG.md already has a section for $new" >&2; exit 1
    fi
    tr -d '\r' < CHANGELOG.md | grep -q '^## Unreleased$' || { echo "CHANGELOG.md has no '## Unreleased' section" >&2; exit 1; }
    printf '%s\n' "$new" > VERSION
    sed -i "s/^## Unreleased\(\r\{0,1\}\)\$/## Unreleased\1\n\1\n## $new ($(date -u +%Y-%m-%d))\1/" CHANGELOG.md
    echo "prepared $new ($(release)): add $(release) to KNOWN in lic/release/src/lib.rs if it is new, review, commit, then tag v$new and push the tag"
    ;;
  *)
    echo "usage: ci/version.sh [release | check [TAG] | notes | prepare X.Y.Z]" >&2
    exit 2
    ;;
esac
