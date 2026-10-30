# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# cmp: the difference line (char or byte), -l, -s, EOF message, skips, statuses.

probe same 'cmp data/fruits.txt data/fruits.txt'
probe differ 'printf '\''apple\nbanAna\ncherry\n'\'' > b; cmp data/fruits.txt b'
probe list 'printf '\''Apple\nbanAna\ncherry\n'\'' > b; cmp -l data/fruits.txt b'
probe silent 'printf '\''x\n'\'' > b; cmp -s data/fruits.txt b; echo $?'
probe eof 'printf '\''apple\n'\'' > b; cmp data/fruits.txt b'
probe eof-list 'printf '\''Apple\n'\'' > b; cmp -l data/fruits.txt b'
probe skip 'printf '\''XXapple\n'\'' > b; cmp b data/fruits.txt 2 0 | head -1; cmp b data/fruits.txt 2'
probe skip-hex-octal 'printf '\''XXapple\n'\'' > b; cmp b data/fruits.txt 0x2; cmp b data/fruits.txt 02'
probe stdin 'cmp - data/fruits.txt < data/words.txt'
probe missing 'cmp data/nosuch data/fruits.txt'
probe directory 'cmp data data/fruits.txt'
probe usage 'cmp data/fruits.txt'
probe l-and-s 'cmp -l -s data/fruits.txt data/words.txt'
probe member 'cmp data/fruits.txt /QSYS.LIB/QGPL.LIB/QCLSRC.FILE/QCLSRC.MBR'
finish
