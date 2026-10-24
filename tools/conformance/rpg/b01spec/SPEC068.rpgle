     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     C                   SETON                                        101112
     C                   SETOFF                                       11
     C                   EVAL      *IN(20) = *ON
     C                   EVAL      *IN21 = *IN(10)
     C                   EVAL      LINE = *IN10 + *IN11 + *IN12 + *IN20 +
     C                             *IN(21)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
