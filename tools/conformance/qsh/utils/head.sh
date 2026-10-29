# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# head: counts, -c, headers for several files, messages.

probe default 'i=1; while [ $i -le 15 ]; do echo $i; i=$((i+1)); done | head'
probe n 'head -n 2 data/fruits.txt; head -n 0 data/fruits.txt; echo "[end]"'
probe number 'head -1 data/fruits.txt'
probe bytes 'head -c 7 data/fruits.txt; echo "|"'
probe several 'head -n 1 data/fruits.txt data/csv.txt'
probe missing 'head data/nosuch data/fruits.txt'
probe bad-count 'head -n x data/fruits.txt; head -n -2 data/fruits.txt'
probe no-newline 'head data/nonl.txt; echo "|"'
probe bad-option 'head -q data/fruits.txt'
finish
