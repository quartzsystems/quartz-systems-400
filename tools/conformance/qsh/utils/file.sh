# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# file: descriptions of text, data, links, special files and /QSYS.LIB objects.

probe text 'file data/fruits.txt data/empty.txt data/nonl.txt'
probe dir-link 'file data data/link; file -h data/link'
probe broken 'ln -s nowhere bl; file bl; file -h bl'
probe utf8 'printf '\''caf\303\251\n'\'' > u; file u'
probe script 'printf '\''#!/bin/sh\necho hi\n'\'' > s; file s'
probe binary 'printf '\''\001\002\003'\'' > b; file b'
probe ebcdic 'printf '\''x'\'' > e; setccsid 37 e; file e'
probe special 'file /dev/null; mkfifo p; file p'
probe qsys 'file /QSYS.LIB/QGPL.LIB /QSYS.LIB/QGPL.LIB/QCLSRC.FILE /QSYS.LIB/QSYS.LIB/QCMD.PGM /QSYS.LIB/QGPL.LIB/QCLSRC.FILE/QCLSRC.MBR'
probe usr-bin 'file /usr/bin/ls /usr/bin/qsh'
probe missing 'file nosuch; echo $?'
probe usage 'file; echo $?'
finish
