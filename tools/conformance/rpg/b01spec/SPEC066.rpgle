     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DC10              S             10    INZ('abcdefghij')
     DC4               S              4    INZ('wxyz')
     DN5               S              5  0 INZ(12345)
     C                   MOVE      C4            C10
     C                   EVAL      LINE = C10
     C                   WRITE     QSYSPRT       LINE
     C                   MOVEL     C4            C10
     C                   EVAL      LINE = C10
     C                   WRITE     QSYSPRT       LINE
     C                   MOVEL(P)  'ab'          C10
     C                   EVAL      LINE = '[' + C10 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   MOVE      N5            C4
     C                   EVAL      LINE = C4
     C                   WRITE     QSYSPRT       LINE
     C                   MOVE      '987'         N5
     C                   EVAL      LINE = %CHAR(N5)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
