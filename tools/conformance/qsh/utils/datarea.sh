# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# datarea: reading and writing char, decimal and logical data areas,
# substrings, *LDA, messages. Data areas are made in the test library.

# QSHUTILS_LIB names the library objects are made in (QSTEST; on
# pub400 the account's own).
L=${QSHUTILS_LIB:-QSTEST}

probe setup 'system -q "CRTDTAARA DTAARA($L/QSHCHR) TYPE(*CHAR) LEN(20) VALUE(CONFIG)"; system -q "CRTDTAARA DTAARA($L/QSHDEC) TYPE(*DEC) LEN(7 2) VALUE(12.5)"; system -q "CRTDTAARA DTAARA($L/QSHLGL) TYPE(*LGL) VALUE('\''1'\'')"; echo $?'
probe read 'datarea /QSYS.LIB/$L.LIB/QSHCHR.DTAARA | od -c | head -3'
probe read-r 'datarea -r /QSYS.LIB/$L.LIB/QSHCHR.DTAARA'
probe substring 'datarea -s 1:3 /QSYS.LIB/$L.LIB/QSHCHR.DTAARA; datarea -s 4 /QSYS.LIB/$L.LIB/QSHCHR.DTAARA | od -c | head -2'
probe write 'datarea -w /QSYS.LIB/$L.LIB/QSHCHR.DTAARA NEWVALUE; echo $?; datarea /QSYS.LIB/$L.LIB/QSHCHR.DTAARA'
probe write-sub 'datarea -w -s 3:2 /QSYS.LIB/$L.LIB/QSHCHR.DTAARA XY; datarea /QSYS.LIB/$L.LIB/QSHCHR.DTAARA'
probe write-long 'datarea -w /QSYS.LIB/$L.LIB/QSHCHR.DTAARA ABCDEFGHIJKLMNOPQRSTUVWXYZ; echo $?; datarea /QSYS.LIB/$L.LIB/QSHCHR.DTAARA'
probe decimal 'datarea /QSYS.LIB/$L.LIB/QSHDEC.DTAARA; datarea -w /QSYS.LIB/$L.LIB/QSHDEC.DTAARA -3.5; datarea /QSYS.LIB/$L.LIB/QSHDEC.DTAARA; datarea -w /QSYS.LIB/$L.LIB/QSHDEC.DTAARA abc; echo $?'
probe logical 'datarea /QSYS.LIB/$L.LIB/QSHLGL.DTAARA; datarea -w /QSYS.LIB/$L.LIB/QSHLGL.DTAARA 0; datarea /QSYS.LIB/$L.LIB/QSHLGL.DTAARA; datarea -w /QSYS.LIB/$L.LIB/QSHLGL.DTAARA 2; echo $?'
probe lda 'datarea '\''*LDA'\'' | wc -c; datarea -w -s 1:5 '\''*LDA'\'' hello; datarea -s 1:8 '\''*LDA'\'''
probe gda 'datarea '\''*GDA'\''; echo $?'
probe missing 'datarea /QSYS.LIB/$L.LIB/QSHNOSUCH.DTAARA; echo $?'
probe usage 'datarea; echo $?; datarea -x x; echo $?; datarea -s 0 x; echo $?'
probe cleanup 'for d in QSHCHR QSHDEC QSHLGL; do system -q "DLTDTAARA DTAARA($L/$d)"; done; echo $?'
finish
