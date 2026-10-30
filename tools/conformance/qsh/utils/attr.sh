# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# attr: every attribute of a stream file, a directory, a member, single
# values, -p, changes, messages.

probe all 'attr data/fruits.txt'
probe dir 'attr data'
probe one 'attr data/fruits.txt CCSID; attr data/fruits.txt OBJTYPE DATA_SIZE'
probe p 'attr -p data/fruits.txt CCSID PC_HIDDEN'
probe link 'attr data/link OBJTYPE; attr -h data/link OBJTYPE'
probe qsys 'attr /QSYS.LIB/QSYS.LIB/QCMD.PGM OBJTYPE CCSID'
probe set 'attr data/fruits.txt PC_HIDDEN=YES; attr data/fruits.txt PC_HIDDEN'
probe set-ccsid 'attr data/fruits.txt CCSID=1208; attr -p data/fruits.txt CCSID CODEPAGE'
probe bad 'attr data/fruits.txt PC_HIDDEN=MAYBE; echo $?; attr data/fruits.txt NOSUCH; echo $?; attr data/fruits.txt DATA_SIZE=5; echo $?'
probe missing 'attr data/nosuch; echo $?'
probe usage 'attr; echo $?; attr -x data; echo $?'
finish
