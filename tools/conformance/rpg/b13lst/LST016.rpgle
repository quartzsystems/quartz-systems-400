      * INDENT with two characters.
      *; run: no
      *; cmd: CRTBNDRPG PGM({lib}/{name}) SRCFILE({lib}/QRPGLESRC) SRCMBR({name}) DBGVIEW(*NONE) OUTPUT(*PRINT) INDENT('| ')
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DI                S              5P 0
     DJ                S              5P 0
     C     1             DO        3             I
     C     I             IFEQ      2
     C                   EVAL      J = J + 10
     C                   ELSE
     C                   EVAL      J = J + 1
     C                   ENDIF
     C                   ENDDO
     C                   EVAL      LINE = %CHAR(J)
     C                   WRITE     QSYSPRT       LINE
     C                   SETON                                        LR
