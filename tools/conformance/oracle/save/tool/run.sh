#!/usr/bin/env bash
# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.

# Run a capture script against an oracle over JDBC.
#   JT400_JAR=/path/to/jt400.jar tools/conformance/oracle/save/tool/run.sh oracle|pub400 OUTDIR SCRIPT
set -euo pipefail
root="$(cd "$(dirname "$0")/../../../../.." && pwd)"
jar="${JT400_JAR:?set JT400_JAR to jt400.jar}"
classes="$root/target/save-tool"
mkdir -p "$classes"
sep=":"; env="$root/tools/conformance/oracle/.env"; cls="$classes"; out="$2"; script="$3"
case "$(uname -s)" in MINGW*|MSYS*|CYGWIN*)
  sep=";"; env="$(cygpath -w "$env")"; cls="$(cygpath -w "$classes")"; jar="$(cygpath -w "$jar")"
  out="$(cygpath -w "$out")"; script="$(cygpath -w "$script")" ;; esac
javac -d "$classes" -cp "$jar" "$root/tools/conformance/oracle/save/tool/Save.java"
# The primary oracle's sign-on server drops connections: start each server directly.
[ "$1" = oracle ] && export QS_SKIP_SIGNON=1
MSYS_NO_PATHCONV=1 java -cp "$cls$sep$jar" Save "$env" "$1" "$out" "$script"
