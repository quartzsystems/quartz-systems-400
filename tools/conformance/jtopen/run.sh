#!/usr/bin/env bash
# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.

# Build the JTOpen suite and run each scenario through the recording proxy.
#   JT400_JAR=/path/to/jt400.jar tools/conformance/jtopen/run.sh [scenario...]
# Traces go to tools/conformance/hostsvr/traces/, results to
# tools/conformance/jtopen/expected/oracle/.
set -euo pipefail
# The primary oracle's sign-on and central servers drop every connection (see
# README); every scenario but those that test the sign-on server starts its
# servers directly. Set QS_HS_SKIP_SIGNON=0 to go through the sign-on server.
export QS_HS_SKIP_SIGNON="${QS_HS_SKIP_SIGNON:-1}"

root="$(cd "$(dirname "$0")/../../.." && pwd)"
jar="${JT400_JAR:?set JT400_JAR to jt400.jar}"
classes="$root/target/jtopen/classes"
traces="$root/tools/conformance/hostsvr/traces"
expected="$root/tools/conformance/jtopen/expected/oracle"
sep=":"
classes_arg="$classes"
case "$(uname -s)" in MINGW*|MSYS*|CYGWIN*) sep=";"; classes_arg="$(cygpath -w "$classes")" ;; esac

mkdir -p "$classes" "$expected"
javac -d "$classes" -cp "$jar" "$root"/tools/conformance/jtopen/src/*.java
cargo build -q -p tn5250-driver
driver="$root/target/debug/tn5250-driver"

all=(signon pwdlevel svrmap cmdcall pgmcall sysval central jdbc dtaq ifs splf)
declare -A class=([signon]=Signon [pwdlevel]=PwdLevel [svrmap]=Svrmap [cmdcall]=CmdCall
  [pgmcall]=PgmCall [sysval]=SysVal [central]=Central [jdbc]=Jdbc [dtaq]=DtaQ [ifs]=Ifs [splf]=Splf)
scenarios=("$@")
[ ${#scenarios[@]} -eq 0 ] && scenarios=("${all[@]}")

for s in "${scenarios[@]}"; do
  echo "== $s"
  find "$traces" -name "$s.trace" -delete 2>/dev/null || true
  "$driver" hostsvr-trace --target oracle --out "$traces" --scenario "$s" -- \
    java -cp "$classes_arg$sep$jar" "${class[$s]}" | tee "$root/target/jtopen/$s.log" |
    grep -E '^(ok|fail|     )' > "$expected/$s.out" || true
  grep -E '\.trace:' "$root/target/jtopen/$s.log" || true
  # The oracle drops new connections while it recycles the last run's
  # prestart jobs; give it a moment between scenarios.
  sleep "${QS_HS_PAUSE:-20}"
done
