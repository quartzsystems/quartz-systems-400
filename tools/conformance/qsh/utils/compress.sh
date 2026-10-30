# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# compress: replacing files, -c -f -v -b, files that would grow, the
# .Z header, messages and statuses.

big() { i=0; while [ $i -lt 500 ]; do echo "line of text $i"; i=$((i + 1)); done; }
probe replace 'big > data/big.txt && compress data/big.txt; echo "status $?"; ls -l data/big*'
probe header 'big | compress | od -c | head -2; big | compress -b 12 | od -c | head -1; printf aaa | compress | od -t x1'
probe verbose 'big > data/big.txt && compress -v data/big.txt; echo "status $?"'
probe grow 'compress data/fruits.txt; echo "status $?"; ls data/fruits*; compress -v data/fruits.txt; echo "status $?"'
probe force 'compress -f -v data/nonl.txt; echo "status $?"; ls -l data/nonl*'
probe stdout 'big > data/big.txt && compress -c data/big.txt | wc -c; ls data/big*'
probe suffix 'echo x > data/a.Z && compress data/a.Z; echo "status $?"'
probe exists 'big > data/big.txt && echo old > data/big.txt.Z && compress data/big.txt; echo "status $?"'
probe bits 'compress -b 8 data/fruits.txt; echo "status $?"; compress -b 17 data/fruits.txt; echo "status $?"'
probe missing 'compress data/nosuch; echo "status $?"; compress data/sub; echo "status $?"'
probe empty 'compress < /dev/null | od -t x1'
probe usage 'compress -q x; echo "status $?"'
finish
