#!/usr/bin/env bash
# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# Runs the netlink crate's live tests as root in a fresh network
# namespace (Linux, sudo). `netns.sh capture` instead rewrites the
# kernel reply fixtures in tests/fixtures/.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../../.." && pwd)"
cd "$root"
export PATH="$HOME/.cargo/bin:$PATH"

as_root() { if [ "$(id -u)" = 0 ]; then "$@"; else sudo "$@"; fi; }

# Build as the user; run only the binaries as root. A root caller passes
# the binaries in NETLINK_TEST_BINS so nothing is built as root.
if [ -n "${NETLINK_TEST_BINS:-}" ]; then
    bins="$NETLINK_TEST_BINS"
else
    bins=$(cargo test -p netlink --tests --no-run --message-format=json 2>/dev/null |
        sed -n 's/.*"executable":"\([^"]*\)".*/\1/p')
fi
[ -n "$bins" ] || { echo "netns.sh: no test binaries built" >&2; exit 1; }

if [ "${1:-}" = capture ]; then
    bin=$(echo "$bins" | grep '/capture-')
    tmp=$(mktemp -d)
    chmod 777 "$tmp"
    as_root unshare -n env QS400_CAPTURE_DIR="$tmp" "$bin" --nocapture
    mkdir -p "$here/fixtures"
    cp "$tmp"/* "$here/fixtures/"
    rm -rf "$tmp"
    echo "netns.sh: fixtures written to $here/fixtures"
    exit 0
fi

for bin in $(echo "$bins" | grep '/live-'); do
    as_root unshare -n env QS400_NETNS=1 "$bin" --test-threads=1 --nocapture
done
