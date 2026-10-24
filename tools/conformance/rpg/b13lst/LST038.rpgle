      /TITLE Listing directives
      *; run: no
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
      /SPACE 3
     DN                S              3P 0 INZ(7)
      /EJECT
     C                   EVAL      LINE = %CHAR(N)
     C                   WRITE     QSYSPRT       LINE
     C                   SETON                                        LR
