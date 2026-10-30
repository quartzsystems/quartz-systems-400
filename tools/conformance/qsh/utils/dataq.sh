# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# dataq: send, receive, peek, clear, count, keys, waits, messages. The
# queues are made in the test library and deleted.

# QSHUTILS_LIB names the library objects are made in (QSTEST; on
# pub400 the account's own).
L=${QSHUTILS_LIB:-QSTEST}

probe setup 'system -q "CRTDTAQ DTAQ($L/QSHQ) MAXLEN(100)"; system -q "CRTDTAQ DTAQ($L/QSHKQ) MAXLEN(100) SEQ(*KEYED) KEYLEN(2)"; echo $?'
probe send-receive 'q=/QSYS.LIB/$L.LIB/QSHQ.DTAQ; dataq $q first entry; dataq -s $q second; dataq $q; dataq -r $q'
probe empty 'dataq -r /QSYS.LIB/$L.LIB/QSHQ.DTAQ; echo $?'
probe wait 'dataq -r -t 1 /QSYS.LIB/$L.LIB/QSHQ.DTAQ; echo $?'
probe count-clear 'q=/QSYS.LIB/$L.LIB/QSHQ.DTAQ; dataq $q a; dataq $q b; dataq -n $q; dataq -c $q; dataq -n $q'
probe peek 'q=/QSYS.LIB/$L.LIB/QSHQ.DTAQ; dataq $q kept; dataq -r -p $q; dataq -n $q; dataq -c $q'
probe keyed 'q=/QSYS.LIB/$L.LIB/QSHKQ.DTAQ; dataq -k K1 $q one; dataq -k K2 $q two; dataq -r -k K2 $q; dataq -r -k K9 $q; echo $?'
probe missing 'dataq -n /QSYS.LIB/$L.LIB/QSHNOQ.DTAQ; echo $?'
probe usage 'dataq; echo $?; dataq -s -c x y; echo $?'
probe cleanup 'system -q "DLTDTAQ DTAQ($L/QSHQ)"; system -q "DLTDTAQ DTAQ($L/QSHKQ)"; echo $?'
finish
