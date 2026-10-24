     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DN                S             10I 0
     C                   CALLB     'COVNOPROC'
     C                   PARM                    N
     C                   EVAL      LINE = 'called'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
