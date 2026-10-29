# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# od: the default format and byte order, -t types, -A, -j, -N, -v,
# column widths, messages.

probe default 'printf abcd | od'
probe chars 'od -c data/fruits.txt'
probe legacy 'printf AB | od -b; printf abcd | od -x; printf abcd | od -d; printf ab | od -o'
probe signed 'printf "\377\377" | od -s'
probe types 'printf hello | od -t x1; printf hello | od -t u1; printf "\377\001" | od -t d1'
probe wide 'printf abcdefgh | od -t x4; printf ab | od -t o2'
probe named 'printf "a\tb\n\000 " | od -t a'
probe escapes 'printf "\a\b\f\r\v\001" | od -t c'
probe several-types 'printf ab | od -t x1 -t c'
probe bases 'printf abcdefghijklmnopq | od -A d -t x1; printf abcdefghijklmnopq | od -A x -t x1; printf abc | od -A n -t x1'
probe skip-count 'printf abcdef | od -j 2 -t c; printf abcdef | od -N 3 -t c; printf abcdef | od -j 1 -N 2 -t c'
probe skip-past 'printf ab | od -j 5'
probe stars 'printf "%032d" 0 | od -t x1; printf "%032d" 0 | od -v -t x1'
probe odd 'printf abc | od -x'
probe empty 'od data/empty.txt'
probe bad 'od -t z data/fruits.txt; od -A q data/fruits.txt'
probe missing 'od data/nosuch'
probe float 'printf "\077\360\000\000\000\000\000\000" | od -t f8; printf "\077\200\000\000" | od -t f4'
finish
