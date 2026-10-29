# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# cat: operands, standard input, errors, /QSYS.LIB members.

probe one 'cat data/fruits.txt'
probe several 'cat data/fruits.txt data/dup.txt'
probe stdin 'echo from stdin | cat'
probe dash 'echo middle | cat data/nonl.txt - data/empty.txt'
probe no-newline 'cat data/nonl.txt; echo "|"'
probe missing 'cat data/nosuch data/fruits.txt'
probe directory 'cat data'
probe unbuffered 'cat -u data/fruits.txt'
probe bad-option 'cat -z data/fruits.txt'
probe link 'cat data/link'
probe member 'cat /QSYS.LIB/QGPL.LIB/QCLSRC.FILE/QCLSRC.MBR 2>&1 | head -3'
probe program 'cat /QSYS.LIB/QSYS.LIB/QCMD.PGM | head -c 20; echo'
probe case-qopensys 'cat /qopensys/NOSUCH'
probe pipe-closed 'cat data/words.txt data/words.txt | head -1'
finish
