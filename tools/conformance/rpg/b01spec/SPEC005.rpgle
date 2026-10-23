     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     C                   SETON                                        0102
     C                   SETOFF                                       03
     C   01
     CAN 02              EVAL      LINE = 'both 01 and 02'
     C   01
     CAN 02              WRITE     QSYSPRT       LINE
     C  N03              EVAL      LINE = 'not 03'
     C  N03              WRITE     QSYSPRT       LINE
     C   03
     COR 02              EVAL      LINE = '03 or 02'
     C   03
     COR 02              WRITE     QSYSPRT       LINE
     C   03              EVAL      LINE = 'never'
     C   03              WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
