     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DN                S              9  2 INZ(10)
     C                   EVAL      N += 5
     C                   EVAL      LINE = %CHAR(N)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      N -= 3
     C                   EVAL      LINE = %CHAR(N)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      N *= 2
     C                   EVAL      LINE = %CHAR(N)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      N /= 4
     C                   EVAL      LINE = %CHAR(N)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      N **= 2
     C                   EVAL      LINE = %CHAR(N)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
