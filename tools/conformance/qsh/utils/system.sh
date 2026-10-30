# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# system: spooled output, message forms and streams, -i -k -n -q -s -v,
# statuses. Objects are made only in the test library and deleted.

# QSHUTILS_LIB names the library objects are made in (QSTEST; on
# pub400 the account's own).
L=${QSHUTILS_LIB:-QSTEST}

probe spooled 'system DSPLIBL | head -5'
probe completion 'system "CRTDTAARA DTAARA($L/QSHPRB) TYPE(*CHAR) LEN(10)"; echo $?; system "DLTDTAARA DTAARA($L/QSHPRB)"; echo $?'
probe escape 'system "DLTDTAARA DTAARA($L/QSHNOSUCH)"; echo $?'
probe escape-streams 'system "DLTDTAARA DTAARA($L/QSHNOSUCH)" 2>/dev/null; echo "--"; system "DLTDTAARA DTAARA($L/QSHNOSUCH)" >/dev/null'
probe no-ids 'system -n "DLTDTAARA DTAARA($L/QSHNOSUCH)"; echo $?'
probe quiet 'system -q "DLTDTAARA DTAARA($L/QSHNOSUCH)"; echo $?'
probe verbose 'system -v "SNDPGMMSG MSG(hello)"'
probe info 'system "SNDPGMMSG MSG('\''an info'\'') MSGTYPE(*INFO)"; echo $?'
probe diag 'system "SNDPGMMSG MSG('\''a diag'\'') MSGTYPE(*DIAG)" 2>&1; echo $?'
probe comp 'system "SNDPGMMSG MSG('\''a comp'\'') MSGTYPE(*COMP)"; echo $?'
probe unknown 'system "FROBNICATE X"; echo $?'
probe same-job 'system -i "DSPJOB OUTPUT(*PRINT)" | head -8'
probe new-job 'system "DSPJOB OUTPUT(*PRINT)" | head -8'
probe no-spool 'system -s DSPLIBL; echo $?'
probe keep 'system -k DSPLIBL >/dev/null; echo $?'
probe words 'system SNDPGMMSG MSG\(joined\); echo $?'
probe usage 'system; echo $?; system -z X; echo $?'
probe options 'for o in b e E i I k K n p q s v; do system -$o "SNDPGMMSG MSG(x)" >/dev/null 2>&1; echo "$o $?"; done'
finish
