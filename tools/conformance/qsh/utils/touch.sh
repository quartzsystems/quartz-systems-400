# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# touch: creating, -c, -t forms, -r, errors.

probe create 'touch new; ls -l new | cut -c1-10'
probe keeps-contents 'touch data/fruits.txt; cat data/fruits.txt'
probe no-create 'touch -c nofile; ls nofile'
probe time 'touch -t 202001021530 data/fruits.txt; ls -l data/fruits.txt'
probe time-seconds 'touch -t 199912312359.59 data/fruits.txt; ls -l data/fruits.txt'
probe time-yy 'touch -t 6901010000 data/fruits.txt; ls -l data/fruits.txt'
probe time-short 'touch -t 03041200 data/fruits.txt; ls -l data/fruits.txt'
probe time-bad 'touch -t 2020130100 data/fruits.txt'
probe reference 'touch -r data/old.txt data/fruits.txt; ls -l data/fruits.txt'
probe reference-missing 'touch -r nosuch data/fruits.txt'
probe a-and-m 'touch -a -m data/fruits.txt; echo $?'
probe missing-dir 'touch nodir/x'
probe usage 'touch'
finish
