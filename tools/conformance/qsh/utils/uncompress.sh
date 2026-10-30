# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# uncompress: names with and without .Z, -c -f -v, damaged input,
# messages and statuses.

big() { i=0; while [ $i -lt 500 ]; do echo "line of text $i"; i=$((i + 1)); done; }
probe restore 'big > data/big.txt && compress data/big.txt && uncompress data/big.txt.Z; echo "status $?"; ls -l data/big*'
probe no-suffix 'big > data/big.txt && compress data/big.txt && uncompress data/big.txt; echo "status $?"; ls data/big*'
probe verbose 'big > data/big.txt && compress data/big.txt && uncompress -v data/big.txt.Z; echo "status $?"'
probe stdout 'big > data/big.txt && compress data/big.txt && uncompress -c data/big.txt.Z | tail -1; ls data/big*'
probe exists 'big > data/big.txt && compress data/big.txt && echo keep > data/big.txt && uncompress data/big.txt.Z; echo "status $?"; uncompress -f data/big.txt.Z; echo "status $?"'
probe not-compressed 'cp data/fruits.txt data/f.Z && uncompress data/f.Z; echo "status $?"; uncompress < data/fruits.txt; echo "status $?"'
probe missing 'uncompress data/nosuch; echo "status $?"; uncompress data/nosuch.Z; echo "status $?"'
probe usage 'uncompress -q x; echo "status $?"'
finish
