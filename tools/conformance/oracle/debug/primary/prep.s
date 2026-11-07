# The programs strdbg.tn debugs at a display: a CL program compiled
# DBGVIEW(*NONE) and the same source with DBGVIEW(*SOURCE), in {work}.
case prep programs for strdbg.tn
src {work} ZZSRC QAPNODBG CLLE
PGM
DCL VAR(&N) TYPE(*DEC) LEN(5 0) VALUE(1)
CHGVAR VAR(&N) VALUE(&N + 1)
ENDPGM
;;
cl CRTBNDCL PGM({work}/QAPNODBG) SRCFILE({work}/ZZSRC) SRCMBR(QAPNODBG) DBGVIEW(*NONE)
cl CRTBNDCL PGM({work}/QAPSRCDBG) SRCFILE({work}/ZZSRC) SRCMBR(QAPNODBG) DBGVIEW(*SOURCE)
