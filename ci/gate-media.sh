#!/usr/bin/env bash
# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# The released media a PTF is for, checked against its release's
# SHA256SUMS: the ISO of the GitHub release v<VERSION>, into DIR. The PTF
# gate installs it (docs/releasing.md "PTFs").
#
#   ci/gate-media.sh DIR
set -euo pipefail
dir=${1:?usage: ci/gate-media.sh DIR}
version=$(ci/version.sh)
mkdir -p "$dir"
gh release download "v$version" -D "$dir" -p 'QS400-*.iso' -p SHA256SUMS --clobber
cd "$dir"
iso=$(ls QS400-*.iso)
[ "$(echo "$iso" | wc -l)" -eq 1 ] || { echo "v$version has more than one ISO" >&2; exit 1; }
grep -E "[ *]$iso\$" SHA256SUMS | sha256sum -c -
echo "release media $iso for v$version"
