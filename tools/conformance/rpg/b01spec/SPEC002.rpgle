     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DA                S              5  0 INZ(10)
     DB                S              5  0 INZ(3)
     DR                S              7  2
     DS                S              3  0
     C     A             ADD       B             R                    101112
     C                   EVAL      LINE = 'ADD ' + %CHAR(R) + ' ' + *IN10 +
     C                             *IN11 + *IN12
     C                   WRITE     QSYSPRT       LINE
     C     A             SUB       B             R                    101112
     C                   EVAL      LINE = 'SUB ' + %CHAR(R) + ' ' + *IN10 +
     C                             *IN11 + *IN12
     C                   WRITE     QSYSPRT       LINE
     C     B             SUB       A             R                    101112
     C                   EVAL      LINE = 'SUB2 ' + %CHAR(R) + ' ' + *IN10 +
     C                             *IN11 + *IN12
     C                   WRITE     QSYSPRT       LINE
     C     A             MULT      B             R
     C                   EVAL      LINE = 'MULT ' + %CHAR(R)
     C                   WRITE     QSYSPRT       LINE
     C     A             DIV       B             R
     C                   EVAL      LINE = 'DIV ' + %CHAR(R)
     C                   WRITE     QSYSPRT       LINE
     C                   Z-ADD     123456        S
     C                   EVAL      LINE = 'ZADD ' + %CHAR(S)
     C                   WRITE     QSYSPRT       LINE
     C                   Z-SUB     5             R
     C                   EVAL      LINE = 'ZSUB ' + %CHAR(R)
     C                   WRITE     QSYSPRT       LINE
     C                   ADD       1             R
     C                   EVAL      LINE = 'ADD1 ' + %CHAR(R)
     C                   WRITE     QSYSPRT       LINE
     C                   Z-ADD(H)  2.555         R
     C                   EVAL      LINE = 'ZADDH ' + %CHAR(R)
     C                   WRITE     QSYSPRT       LINE
     C     A             ADD       0             S                        13
     C                   EVAL      LINE = 'IND13 ' + *IN13
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
