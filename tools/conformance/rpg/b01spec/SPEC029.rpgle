     FQSYSPRT   O    F  132        PRINTER
     F                                     OFLIND(*INOF)
     DLINE             DS           132
     C                   EVAL      LINE = 'of=' + *INOF
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
