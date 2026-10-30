#!/usr/bin/env bash
# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# Run the PASE mechanism test as root: new user, mount and PID
# namespaces need it. WSL: wsl -d Ubuntu -u root -- bash THIS. The build
# runs as BUILD_USER (default: the owner of the repository); the probe
# is built for x86_64-unknown-linux-musl. QS400_TARGET_DIR relocates
# the build (default: ~BUILD_USER/qs400-target-pase).
set -euo pipefail

repo=$(cd "$(dirname "$0")/../../.." && pwd)
user=${BUILD_USER:-$(stat -c %U "$repo/Cargo.toml")}
[ "$user" = root ] && user=$(ls /home | head -1)
home=$(getent passwd "$user" | cut -d: -f6)
target=${QS400_TARGET_DIR:-$home/qs400-target-pase}

if [ "$(id -u)" != 0 ]; then
    echo "run as root" >&2
    exit 1
fi

as_user() {
    runuser -u "$user" -- env PATH="$home/.cargo/bin:$PATH" HOME="$home" \
        CARGO_TARGET_DIR="$target" bash -c "cd '$repo' && $1"
}

as_user "cargo build -p pase-probe --release --target x86_64-unknown-linux-musl"
probe="$target/x86_64-unknown-linux-musl/release/pase-probe"
[ -x "$probe" ] || { echo "pase-probe not built" >&2; exit 1; }
bin=$(as_user "cargo test -p pase --test linux --no-run 2>&1" |
    sed -n 's/.*Executable tests[/\]linux.rs (\(.*\))/\1/p' | tail -1)
[ -x "$bin" ] || { echo "test binary not found" >&2; exit 1; }
PASE_PROBE="$probe" "$bin" --nocapture
