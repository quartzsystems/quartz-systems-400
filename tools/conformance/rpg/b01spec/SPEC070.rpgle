     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DA                S              3  0 INZ(5)
     DB                S              3  0 INZ(5)
     C     A             COMP      B                                  101112
     C                   EVAL      LINE = 'eq ' + *IN10 + *IN11 + *IN12
     C                   WRITE     QSYSPRT       LINE
     C     A             COMP      9                                  101112
     C                   EVAL      LINE = 'lt ' + *IN10 + *IN11 + *IN12
     C                   WRITE     QSYSPRT       LINE
     C     'b'           COMP      'a'                                101112
     C                   EVAL      LINE = 'gt ' + *IN10 + *IN11 + *IN12
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
