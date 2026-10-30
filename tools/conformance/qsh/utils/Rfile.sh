# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# Rfile: reading and writing a source member by path and qualified name,
# trailing blanks, -a -b -K -Q -c, messages. The file is made in the test
# library and deleted.

# QSHUTILS_LIB names the library objects are made in (QSTEST; on
# pub400 the account's own).
L=${QSHUTILS_LIB:-QSTEST}

probe setup 'system -q "CRTSRCPF FILE($L/QSHSRC) RCDLEN(92)"; system -q "ADDPFM FILE($L/QSHSRC) MBR(M1)"; echo $?'
probe write 'printf '\''one\ntwo\n'\'' | Rfile -w /QSYS.LIB/$L.LIB/QSHSRC.FILE/M1.MBR; echo $?'
probe read 'Rfile -r /QSYS.LIB/$L.LIB/QSHSRC.FILE/M1.MBR | od -c | head -4'
probe keep 'Rfile -rK /QSYS.LIB/$L.LIB/QSHSRC.FILE/M1.MBR | od -c | head -4'
probe binary 'Rfile -rb /QSYS.LIB/$L.LIB/QSHSRC.FILE/M1.MBR | od -c | head -4'
probe append 'printf '\''three\n'\'' | Rfile -wa /QSYS.LIB/$L.LIB/QSHSRC.FILE/M1.MBR; Rfile -r /QSYS.LIB/$L.LIB/QSHSRC.FILE/M1.MBR'
probe qualified 'Rfile -rQ "$L/QSHSRC(M1)"; Rfile -rQ "$L/QSHSRC"; Rfile -rQ "*LIBL/QSHSRC"; echo $?'
probe file-dir 'Rfile -r /QSYS.LIB/$L.LIB/QSHSRC.FILE; echo $?'
probe command 'Rfile -c "OVRDBF FILE(QSHSRC) TOFILE($L/QSHSRC) MBR(M1)" -r /QSYS.LIB/$L.LIB/QSHSRC.FILE/M1.MBR; echo $?'
probe long 'printf '\''%0100d\n'\'' 1 | Rfile -w /QSYS.LIB/$L.LIB/QSHSRC.FILE/M1.MBR; echo $?; Rfile -r /QSYS.LIB/$L.LIB/QSHSRC.FILE/M1.MBR'
probe missing 'Rfile -r /QSYS.LIB/$L.LIB/QSHSRC.FILE/NOSUCH.MBR; echo $?; Rfile -rQ "$L/NOSUCH"; echo $?'
probe usage 'Rfile; echo $?; Rfile -rw x; echo $?'
probe cleanup 'system -q "DLTF FILE($L/QSHSRC)"; echo $?'
finish
