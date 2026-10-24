#!/usr/bin/env bash
# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# Run the LinuxBackend tests (ignored by default) over a fresh xfs loop
# image. Run as root (WSL: wsl -d Ubuntu -u root -- bash THIS); the
# build runs as BUILD_USER (default: the owner of the repository).
# Needs xfsprogs (apt install xfsprogs). QS400_TARGET_DIR relocates the
# build (default: ~BUILD_USER/qs400-target-ifs).
set -euo pipefail

repo=$(cd "$(dirname "$0")/../../.." && pwd)
user=${BUILD_USER:-$(stat -c %U "$repo/Cargo.toml")}
[ "$user" = root ] && user=$(ls /home | head -1)
home=$(getent passwd "$user" | cut -d: -f6)
target=${QS400_TARGET_DIR:-$home/qs400-target-ifs}

if [ "$(id -u)" != 0 ]; then
    echo "run as root" >&2
    exit 1
fi
command -v mkfs.xfs >/dev/null || { echo "mkfs.xfs missing: apt install xfsprogs" >&2; exit 1; }

# Build as the user, and find the test binary.
bin=$(runuser -u "$user" -- env PATH="$home/.cargo/bin:$PATH" HOME="$home" \
    CARGO_TARGET_DIR="$target" \
    bash -c "cd '$repo' && cargo test -p ifs --test backend --no-run 2>&1" |
    sed -n 's/.*Executable tests[/\\]backend.rs (\(.*\))/\1/p' | tail -1)
[ -x "$bin" ] || { echo "test binary not found" >&2; exit 1; }

work=$(mktemp -d /tmp/qs400-ifs.XXXXXX)
cleanup() {
    umount "$work/mnt" 2>/dev/null || true
    rm -rf "$work"
}
trap cleanup EXIT
truncate -s 512M "$work/xfs.img"
mkfs.xfs -q -f "$work/xfs.img"
mkdir "$work/mnt"
mount -o loop "$work/xfs.img" "$work/mnt"

status=0
for t in linux_backend_conformance ifs_over_linux; do
    mkdir "$work/mnt/$t"
    QS400_IFS_TEST_DIR="$work/mnt/$t" "$bin" --ignored --exact "$t" || status=1
done
exit $status
