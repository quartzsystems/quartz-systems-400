     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DC4               S              4A
     DC8               S              8A
     DC12              S             12A
     DP7               S              7P 2 INZ(-12345.67)
     DP5               S              5P 0 INZ(678)
     DB4               S              4B 0 INZ(-42)
     DI10              S             10I 0 INZ(-1234567)
     DU5               S              5U 0 INZ(65000)
     DI3               S              3I 0 INZ(7)
     C                   EVAL      C8 = 'abcdefgh'
     C                   MOVE      P7            C8
     C                   EVAL      LINE = 'a [' + C8 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C4 = 'abcd'
     C                   MOVEL     P7            C4
     C                   EVAL      LINE = 'b [' + C4 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C8 = 'abcdefgh'
     C                   MOVE      P5            C8
     C                   EVAL      LINE = 'c [' + C8 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C8 = 'abcdefgh'
     C                   MOVE      B4            C8
     C                   EVAL      LINE = 'd [' + C8 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C12 = 'abcdefghijkl'
     C                   MOVE      I10           C12
     C                   EVAL      LINE = 'e [' + C12 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C4 = 'abcd'
     C                   MOVEL     I10           C4
     C                   EVAL      LINE = 'f [' + C4 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C8 = 'abcdefgh'
     C                   MOVE      U5            C8
     C                   EVAL      LINE = 'g [' + C8 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C8 = 'abcdefgh'
     C                   MOVE(P)   I3            C8
     C                   EVAL      LINE = 'h [' + C8 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C8 = 'abcdefgh'
     C                   MOVEL(P)  B4            C8
     C                   EVAL      LINE = 'i [' + C8 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
