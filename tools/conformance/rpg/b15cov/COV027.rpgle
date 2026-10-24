     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     C                   SHTDN                                        99
     C                   EVAL      LINE = 'shtdn ' + *IN99 + ' ' + %SHTDN
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
