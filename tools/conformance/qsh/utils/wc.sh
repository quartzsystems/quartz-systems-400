# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# wc: the column widths, -c -m -l -w, totals, messages.

probe all 'wc data/words.txt'
probe lines 'wc -l data/fruits.txt; wc -w data/words.txt; wc -c data/fruits.txt'
probe lw 'wc -lw data/words.txt'
probe total 'wc data/fruits.txt data/csv.txt'
probe stdin 'printf "one two\nthree\n" | wc'
probe dash 'echo a b | wc - data/fruits.txt'
probe no-newline 'wc data/nonl.txt; wc data/empty.txt'
probe missing 'wc data/nosuch data/fruits.txt'
probe chars 'printf "h\303\251\n" > u.txt; wc -m u.txt; wc -c u.txt; wc -c -m u.txt'
probe bad-option 'wc -x'
probe blanks 'printf "   \n\t\n" | wc'
probe directory 'wc data/sub'
probe member 'wc -l /QSYS.LIB/QGPL.LIB/QCLSRC.FILE/QCLSRC.MBR'
finish
