# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# sort: collation, keys, -n -r -u -t -f -b -d -i, -c, -o, messages.
# The plain order shows whether Qshell sorts by byte or by CCSID.

probe plain 'sort data/words.txt; sort data/mixed.txt'
probe fold 'sort -f data/mixed.txt'
probe reverse 'sort -r data/fruits.txt'
probe numeric 'sort -n data/nums.txt; sort -nr data/nums.txt'
probe text-numbers 'sort data/nums.txt'
probe unique 'sort -u data/dup.txt; sort -uf data/mixed.txt'
probe key 'sort -k 2 data/people.txt'
probe key-n 'sort -k 2,2n -k 1,1 data/people.txt'
probe key-nr 'sort -k 2,2nr -k 1,1 data/people.txt'
probe key-ties 'sort -k 3,3 data/people.txt'
probe separator 'sort -t, -k 2 data/csv.txt; sort -t, -k3r data/csv.txt'
probe key-char 'printf "xa2\nyb1\nzc3\n" | sort -k 1.3'
probe key-end-char 'printf "ab9\nab1\naa5\n" | sort -k 1.1,1.2'
probe blanks 'printf "  b\na\n c\n" | sort -b; printf "  b\na\n c\n" | sort'
probe key-blanks 'printf "x   b\nx a\nx  c\n" | sort -k 2b; printf "x   b\nx a\nx  c\n" | sort -k 2'
probe dictionary 'printf "b-c\nb.a\nbb\n" | sort -d'
probe check 'sort -c data/fruits.txt; echo $?; sort -c data/words.txt; echo $?'
probe check-unique 'sort -cu data/dup.txt; echo $?'
probe check-stdin 'printf "a\nc\nb\n" | sort -c'
probe output 'sort -o data/words.txt data/words.txt; cat data/words.txt'
probe merge 'sort -m data/fruits.txt data/csv.txt'
probe missing 'sort data/nosuch'
probe bad-key 'sort -k 0 data/fruits.txt; sort -k 1.0 data/fruits.txt'
probe bad-separator 'sort -t ab data/fruits.txt'
probe bad-option 'sort -Q data/fruits.txt'
probe signs 'printf "1.5\n-2\n+3\n.5\n-0.5\n0\n" | sort -n'
probe non-numbers 'printf "x\n1\n-1\ny\n" | sort -n'
probe numeric-unique 'printf "01\n1\n2\n" | sort -nu; printf -- "-0\n0\n" | sort -nu'
probe empty-lines 'printf "b\n\na\n" | sort'
probe global-flags 'printf "2 b\n10 a\n" | sort -n -k 2; printf "x 2\ny 10\n" | sort -n -k 2'
probe unique-key 'printf "a 2\na 1\nb 3\n" | sort -k1,1 -u'
probe reverse-fold 'printf "a\nA\n" | sort -fr'
probe case-order 'printf "b\nB\na\nA\n1\n_\n" | sort'
finish
