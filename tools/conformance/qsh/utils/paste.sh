# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# paste: side by side, -s, -d lists and escapes, standard input twice.

probe two 'paste data/fruits.txt data/csv.txt'
probe delim 'paste -d: data/fruits.txt data/csv.txt'
probe delim-list 'paste -d ":," data/fruits.txt data/csv.txt data/dup.txt'
probe serial 'paste -s data/fruits.txt; paste -s -d, data/fruits.txt data/csv.txt'
probe uneven 'paste data/fruits.txt data/dup.txt'
probe stdin-twice 'printf "a\nb\nc\nd\ne\n" | paste - -'
probe escapes 'paste -d "\t\n" data/fruits.txt data/csv.txt; paste -d "\0" data/fruits.txt data/csv.txt'
probe missing 'paste data/nosuch data/fruits.txt'
finish
