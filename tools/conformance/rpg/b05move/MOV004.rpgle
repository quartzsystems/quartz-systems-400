     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DC3               S              3A
     DC6               S              6A
     DC10              S             10A
     DZ5               S              5S 0 INZ(12345)
     DZN               S              5S 0 INZ(-12345)
     DZ72              S              7S 2 INZ(-123.45)
     C                   EVAL      C6 = 'abcdef'
     C                   MOVEL     Z5            C6
     C                   EVAL      LINE = 'a [' + C6 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C6 = 'abcdef'
     C                   MOVEL     ZN            C6
     C                   EVAL      LINE = 'b [' + C6 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C3 = 'abc'
     C                   MOVEL     Z5            C3
     C                   EVAL      LINE = 'c [' + C3 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C3 = 'abc'
     C                   MOVEL     ZN            C3
     C                   EVAL      LINE = 'd [' + C3 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C10 = 'abcdefghij'
     C                   MOVEL(P)  ZN            C10
     C                   EVAL      LINE = 'e [' + C10 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C10 = 'abcdefghij'
     C                   MOVEL     Z72           C10
     C                   EVAL      LINE = 'f [' + C10 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C3 = 'abc'
     C                   MOVEL(P)  Z72           C3
     C                   EVAL      LINE = 'g [' + C3 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
