# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# Shared by the utility probes. A probe is run in QSH on the oracle:
#
#   QSH CMD('cd /home/QSTEST/qshutils && . ./_lib.sh && . ./ls.sh')
#
# with QIBM_QSH_CMD_OUTPUT=FILE=/home/QSTEST/qshutils/out/ls.txt. On
# pub400 the directory is the account's home ({user}). Each case prints
# `### name`, its standard output and error (merged), then `status N`.
# Everything is made under $T, which `finish` removes; nothing outside
# it is changed.

P=${QSHUTILS_DIR:-/home/QSTEST/qshutils}
T=$P/work.$$
exec 2>&1

# probe NAME COMMAND: run COMMAND in $T and report it.
probe() {
    echo "### $1"
    (cd "$T" && eval "$2")
    echo "status $?"
}

# The seeded tree the Quartz corpus uses (qsh/qsh-utils/tests/common).
seed() {
    rm -rf "$T"
    mkdir -p "$T/data/sub/inner" "$T/bin" || exit 1
    cd "$T" || exit 1
    printf 'apple\nbanana\ncherry\n' > data/fruits.txt
    printf '10\n9\n100\n-5\n2.5\n' > data/nums.txt
    printf 'the quick brown fox\njumps over\nthe lazy dog\n' > data/words.txt
    : > data/empty.txt
    printf 'no newline' > data/nonl.txt
    printf 'a\tb\tc\n1\t2\t3\n' > data/tabs.txt
    printf 'x,1,b\ny,2,a\nz,3,c\n' > data/csv.txt
    printf 'a\na\nb\nc\nc\nc\nd\n' > data/dup.txt
    printf 'bob 25 ny\nalice 30 la\ncarol 25 sf\ndave 19 ny\n' > data/people.txt
    printf 'Apple\nbanana\nCherry\napple\n' > data/mixed.txt
    printf 'deep\n' > data/sub/deep.txt
    printf 'leaf\n' > data/sub/inner/leaf
    printf 'hidden\n' > data/.hidden
    printf 'old\n' > data/old.txt
    touch -t 202009131226 data/old.txt
    ln -s fruits.txt data/link
    : > bin/tool
    chmod 755 bin/tool
    cd "$P" || exit 1
}

finish() {
    cd "$P" && rm -rf "$T"
}

seed
