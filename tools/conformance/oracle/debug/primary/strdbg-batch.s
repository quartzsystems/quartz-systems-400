# STRDBG on the primary (V7R1M0): a CL program compiled DBGVIEW(*NONE),
# and the same source compiled DBGVIEW(*SOURCE). Programs QAPNODBG and
# QAPSRCDBG in {work}, deleted at the end.
case strdbg-batch STRDBG of programs with and without debug data
boot
src {work} ZZSRC QAPNODBG CLLE
PGM
DCL VAR(&N) TYPE(*DEC) LEN(5 0) VALUE(1)
CHGVAR VAR(&N) VALUE(&N + 1)
ENDPGM
;;
cl CRTBNDCL PGM({work}/QAPNODBG) SRCFILE({work}/ZZSRC) SRCMBR(QAPNODBG) DBGVIEW(*NONE)
cl STRDBG PGM({work}/QAPNODBG) UPDPROD(*YES)
cl ENDDBG
cl CRTBNDCL PGM({work}/QAPSRCDBG) SRCFILE({work}/ZZSRC) SRCMBR(QAPNODBG) DBGVIEW(*SOURCE)
cl STRDBG PGM({work}/QAPSRCDBG) UPDPROD(*YES)
cl ENDDBG
cl DLTPGM PGM({work}/QAPNODBG)
cl DLTPGM PGM({work}/QAPSRCDBG)
