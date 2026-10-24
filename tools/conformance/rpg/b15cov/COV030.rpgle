     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DN                S              3P 0
     C     N             WHENEQ    1
     C                   EVAL      LINE = 'x'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
