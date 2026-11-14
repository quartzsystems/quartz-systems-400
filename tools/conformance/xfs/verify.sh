#!/bin/bash
# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# Checks xfsfmt's output with the Linux tools and kernel (WSL Ubuntu is
# fine). Two steps, because mounting needs root and cargo does not:
#
#   tools/conformance/xfs/verify.sh build DIR   # as yourself: write images
#   tools/conformance/xfs/verify.sh big DIR     # 1 TiB and 64 TiB images
#   tools/conformance/xfs/verify.sh check DIR   # as root: repair, mount, use
#
# big's DIR must hold sparse files past ext4's 16 TiB: a tmpfs (/tmp in
# WSL) or an xfs. check needs xfsprogs (apt-get install -y xfsprogs).
# Every image must pass xfs_repair -n before and after a read-write mount
# that creates, fills, links, renames and deletes files. A reflink image
# (*-reflink.img) also clones a file (FICLONE and a range clone), checks
# the clone's extents are shared, writes to it and checks the original
# is unchanged.

set -u
mode=${1:?build, big or check}
dir=${2:?image directory}
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../.." && pwd)

if [ "$mode" = build ] || [ "$mode" = big ]; then
    mkdir -p "$dir"
    cd "$repo" || exit 1
    if [ "$mode" = build ]; then
        QS400_XFS_DIR="$dir" cargo test -q -p xfsfmt --test images -- --ignored --exact write_images --nocapture
    else
        QS400_XFS_BIG_DIR="$dir" cargo test -q -p xfsfmt --test images -- --ignored --exact write_big_images --nocapture
    fi
    exit $?
fi

[ "$mode" = check ] || { echo "mode is build, big or check"; exit 2; }
[ "$(id -u)" = 0 ] || { echo "check runs as root"; exit 2; }

fail=0
bad() { echo "FAIL $1: $2"; fail=1; }

repair() {
    local out
    out=$(xfs_repair -n -f "$1" 2>&1)
    local rc=$?
    # Phase banners and the image-file notice only; any other line is
    # a finding.
    local findings
    findings=$(echo "$out" | grep -vE '^(Phase [0-9]| +- |No modify flag set|Cannot get host filesystem geometry|Repair may fail if there is a sector size|the image and the host filesystem|$)')
    if [ $rc -ne 0 ] || [ -n "$findings" ]; then
        bad "$1" "xfs_repair -n ($2) exit $rc"
        echo "$out" | sed 's/^/    /'
    fi
}

traffic() {
    local m=$1
    rm -rf "$m/home/QSTEST"
    mkdir -p "$m/QIBM/ProdData" "$m/home/QSTEST/a/b/c" || return 1
    dd if=/dev/urandom of="$m/home/QSTEST/big" bs=1M count=24 status=none || return 1
    for i in $(seq 1 500); do echo "member $i" > "$m/home/QSTEST/a/f$i.txt"; done
    ln "$m/home/QSTEST/big" "$m/home/QSTEST/big.hard" || return 1
    ln -s ../big "$m/home/QSTEST/a/big.sym" || return 1
    setfattr -n trusted.qs400.ccsid -v 1208 "$m/home/QSTEST/big" 2>/dev/null || true
    mv "$m/home/QSTEST/a/f1.txt" "$m/home/QSTEST/a/b/renamed.txt" || return 1
    sync
    rm -f "$m"/home/QSTEST/a/f1[0-9]*.txt "$m/home/QSTEST/big.hard"
    rmdir "$m/home/QSTEST/a/b/c" || return 1
    sync
    cmp -s "$m/home/QSTEST/big" "$m/home/QSTEST/a/big.sym" || return 1
}

# Clones: a whole file and a range, shared until written. Leaves the
# original's digest in the file $2 for the read-only remount.
reflink_traffic() {
    local d=$1/home/QSTEST/clone
    mkdir -p "$d" || return 1
    dd if=/dev/urandom of="$d/disk.raw" bs=1M count=8 status=none || return 1
    sync
    cp --reflink=always "$d/disk.raw" "$d/disk.copy" || { echo "    FICLONE refused"; return 1; }
    xfs_io -f -c "reflink $d/disk.raw 2m 0 2m" "$d/part" > /dev/null || return 1
    sync
    filefrag -v "$d/disk.copy" | grep -q shared || { echo "    copy not shared"; return 1; }
    filefrag -v "$d/part" | grep -q shared || { echo "    range not shared"; return 1; }
    cmp -s "$d/disk.raw" "$d/disk.copy" || return 1
    cmp -s -i 2097152:0 -n 2097152 "$d/disk.raw" "$d/part" || return 1
    sha256sum < "$d/disk.raw" > "$2"
    dd if=/dev/zero of="$d/disk.copy" bs=4k count=8 seek=100 conv=notrunc status=none || return 1
    dd if=/dev/zero of="$d/part" bs=4k count=1 conv=notrunc status=none || return 1
    sync
    cmp -s "$d/disk.raw" "$d/disk.copy" && { echo "    write reached no copy"; return 1; }
    [ "$(sha256sum < "$d/disk.raw")" = "$(cat "$2")" ] || { echo "    original changed"; return 1; }
    xfs_io -c "fiemap -v" "$d/disk.copy" | sed -n '2,6s/^/    /p'
}

for img in "$dir"/*.img; do
    echo "== $img"
    label=$(xfs_admin -l "$img" 2>/dev/null)
    [ "$label" = 'label = "QS400-IFS"' ] || bad "$img" "label: $label"
    # The features the name promises, and only those.
    reflink=0
    case $img in *-reflink.img) reflink=1 ;; esac
    feats=$(xfs_db -r -c version "$img" 2>/dev/null)
    has=0
    echo "$feats" | grep -q 'RMAPBT' && echo "$feats" | grep -q 'REFLINK' && has=1
    [ "$has" = "$reflink" ] || bad "$img" "features: $feats"
    repair "$img" "fresh"
    sum=$(mktemp)
    m=$(mktemp -d)
    seen=$(dmesg | wc -l)
    if mount -t xfs -o loop,nodev,nosuid,noexec "$img" "$m"; then
        grep -q " $m xfs rw" /proc/mounts || bad "$img" "not mounted read-write"
        df -B1 --output=size,avail "$m" | tail -1 | sed 's/^/    size, avail: /'
        traffic "$m" || bad "$img" "file traffic"
        if [ "$reflink" = 1 ]; then
            reflink_traffic "$m" "$sum" || bad "$img" "reflink traffic"
        fi
        umount "$m" || bad "$img" "umount"
        repair "$img" "after use"
        if mount -t xfs -o loop,ro "$img" "$m"; then
            [ "$(cat "$m/home/QSTEST/a/b/renamed.txt")" = "member 1" ] || bad "$img" "contents lost"
            if [ "$reflink" = 1 ]; then
                [ "$(sha256sum < "$m/home/QSTEST/clone/disk.raw")" = "$(cat "$sum")" ] || bad "$img" "clone's original changed"
                filefrag -v "$m/home/QSTEST/clone/disk.copy" | grep -q shared || bad "$img" "sharing lost"
            fi
            umount "$m"
        else
            bad "$img" "remount"
        fi
    else
        bad "$img" "mount"
        dmesg | tail -5 | sed 's/^/    /'
    fi
    rmdir "$m"
    rm -f "$sum"
    # The kernel must not have complained about anything.
    kmsg=$(dmesg | tail -n +$((seen + 1)) | grep XFS | grep -viE "Mounting V5|Ending clean mount|Unmounting Filesystem")
    [ -z "$kmsg" ] || { bad "$img" "kernel messages"; echo "$kmsg" | sed "s/^/    /"; }
done

[ $fail = 0 ] && echo "all images clean"
exit $fail
