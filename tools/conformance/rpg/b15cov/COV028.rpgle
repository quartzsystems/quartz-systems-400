     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     C                   SHTDN
     C                   EVAL      LINE = 'x'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
