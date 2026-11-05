#!/usr/bin/env bash
# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.

# Capture cases from their oracle into tools/conformance/sql/expected/.
#   tools/conformance/sql/capture.sh [CASE.sql...]     (default: every uncaptured case)
# QS_SQL_RECAPTURE=1 captures cases that already have an outcome too.
set -euo pipefail
root="$(cd "$(dirname "$0")/../../.." && pwd)"
cd "$root"
jar="${JT400_JAR:-$root/target/jtopen/jt400.jar}"
[ -f "$jar" ] || { echo "no jt400.jar at $jar (set JT400_JAR)"; exit 2; }
classes="$root/target/sqlkit/classes"
sep=":"; cp_classes="$classes"; cp_jar="$jar"
case "$(uname -s)" in MINGW*|MSYS*|CYGWIN*) sep=";"; cp_classes="$(cygpath -w "$classes")"; cp_jar="$(cygpath -w "$jar")" ;; esac
mkdir -p "$classes"
javac -d "$classes" tools/conformance/sql/oracle/SqlKit.java

cases=("$@")
[ ${#cases[@]} -eq 0 ] && mapfile -t cases < <(find tools/conformance/sql/cases -name '*.sql' | sort)
declare -A want
for c in "${cases[@]}"; do
  area="$(basename "$(dirname "$c")")"; name="$(basename "$c" .sql)"
  out="tools/conformance/sql/expected/$area/$name.out"
  [ -f "$out" ] && [ "${QS_SQL_RECAPTURE:-0}" != 1 ] && continue
  oracle="$(sed -n 's/^-- oracle: *\([a-z0-9]*\).*/\1/p' "$c" | head -1)"
  case "$oracle" in primary|pub400) want[$oracle]+="$c"$'\n' ;; *) echo "skip $c (oracle: $oracle)" ;; esac
done
for o in primary pub400; do
  [ -n "${want[$o]:-}" ] || continue
  mapfile -t list < <(printf '%s' "${want[$o]}")
  java -cp "$cp_classes$sep$cp_jar" SqlKit "$o" tools/conformance/sql/expected "${list[@]}"
done
