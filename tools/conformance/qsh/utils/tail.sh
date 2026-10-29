# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# tail: counts, +n, -c, headers, the old forms, messages.

probe default 'i=1; while [ $i -le 15 ]; do echo $i; i=$((i+1)); done | tail'
probe n 'tail -n 2 data/fruits.txt; tail -n 0 data/fruits.txt; echo "[end]"'
probe from 'tail -n +2 data/fruits.txt; tail +2 data/fruits.txt; tail -n +0 data/fruits.txt'
probe number 'tail -1 data/fruits.txt'
probe bytes 'tail -c 4 data/fruits.txt; tail -c +3 data/csv.txt'
probe several 'tail -n 1 data/fruits.txt data/csv.txt'
probe missing 'tail data/nosuch'
probe bad-count 'tail -n abc data/fruits.txt'
probe no-newline 'printf "a\nb\nc" | tail -n 2; echo "|"'
probe follow-not-terminal 'tail -f -n 1 data/fruits.txt & sleep 2; kill $! 2>/dev/null; wait'
finish
