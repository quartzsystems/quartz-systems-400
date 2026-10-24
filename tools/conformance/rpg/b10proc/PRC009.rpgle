      *; with: PRC009P
      *; setup: CRTBNDRPG PGM({lib}/PRC009P) SRCFILE({lib}/QRPGLESRC) SRCMBR(PRC009P) DBGVIEW(*NONE)
      *; cleanup: DLTPGM PGM({lib}/PRC009P)
     FQSYSPRT   O    F  132        PRINTER
     D line            DS           132
     D x               S              5P 0 INZ(7)
     D y               S              8    INZ('before')
     D r               S              5P 0
     D s               S              8
     C                   CALL      'PRC009P'
     C     r             PARM      x             P1                5 0
     C     s             PARM      y             P2                8
     C                   EVAL      line = %char(x) + ' ' + y + ' ' + %char(r) +
     C                             ' ' + s
     C                   WRITE     QSYSPRT       line
     C     PL            PLIST
     C                   PARM                    x
     C                   CALL      'PRC009P'     PL
     C                   EVAL      line = %char(x)
     C                   WRITE     QSYSPRT       line
     C                   EVAL      *INLR = *ON
