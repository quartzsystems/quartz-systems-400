# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# split: lines, bytes, prefixes, suffix length, messages.

probe lines 'i=1; while [ $i -le 5 ]; do echo $i; i=$((i+1)); done > f; split -l 2 f; ls x*; cat xac'
probe default 'i=1; while [ $i -le 1001 ]; do echo $i; i=$((i+1)); done > f; split f; wc -l xaa xab'
probe bytes 'printf abcdefgh > f; split -b 3 f; cat xaa; echo; cat xac; echo'
probe bytes-k 'i=0; while [ $i -lt 30 ]; do echo "a line of text that is long enough to count"; i=$((i+1)); done > f; split -b 1k f; wc -c xaa xab'
probe prefix 'split -l 1 data/fruits.txt part; ls part*'
probe suffix 'split -a 3 -l 1 data/fruits.txt; ls x*'
probe old-form 'split -2 data/fruits.txt; cat xab'
probe exhausted 'i=1; while [ $i -le 30 ]; do echo $i; i=$((i+1)); done > f; split -a 1 -l 1 f; echo $?; ls x* | wc -l'
probe missing 'split data/nosuch'
probe bad-count 'split -l 0 data/fruits.txt'
probe too-many 'split a b c'
finish
