# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# cut: bytes, characters, fields, -s, list forms, messages.

probe chars 'cut -c 2-4 data/fruits.txt; cut -c 1,3 data/fruits.txt'
probe open 'cut -c -2 data/fruits.txt; cut -c 4- data/fruits.txt'
probe bytes 'cut -b 1-3 data/csv.txt'
probe fields 'cut -d, -f2 data/csv.txt; cut -d, -f1,3 data/csv.txt; cut -d, -f2- data/csv.txt'
probe tab 'cut -f2 data/tabs.txt'
probe no-delim 'printf "a,b\nnone\n" | cut -d, -f2'
probe suppress 'printf "a,b\nnone\n" | cut -s -d, -f2'
probe past-end 'cut -d, -f5 data/csv.txt'
probe empty-fields 'echo "a,,c" | cut -d, -f2,3'
probe usage 'cut data/fruits.txt'
probe two-lists 'cut -c1 -f1 data/fruits.txt'
probe bad-list 'cut -c 0 data/fruits.txt; cut -c 3-1 data/fruits.txt'
probe long-delim 'cut -d ab -f1 data/fruits.txt'
probe missing 'cut -c1 data/nosuch'
probe overlap 'cut -c 1-3,2-4 data/fruits.txt'
probe space-list 'cut -c "1 3" data/fruits.txt'
probe no-newline 'cut -c 1-2 data/nonl.txt'
finish
