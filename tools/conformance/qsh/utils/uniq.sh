# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# uniq: -c column width, -d -u -f -s, old forms, output file, messages.

probe plain 'uniq data/dup.txt'
probe count 'uniq -c data/dup.txt'
probe repeated 'uniq -d data/dup.txt; uniq -u data/dup.txt'
probe fields 'printf "x a\ny a\nz b\n" | uniq -f 1'
probe chars 'printf "xa\nya\nzb\n" | uniq -s 1'
probe old-forms 'printf "x a\ny a\nz b\n" | uniq -1; printf "xa\nya\nzb\n" | uniq +1'
probe output 'uniq data/dup.txt out; cat out'
probe missing 'uniq data/nosuch'
probe too-many 'uniq a b c'
probe no-newline 'printf "a\na" | uniq'
probe bad-count 'uniq -f x data/dup.txt'
finish
