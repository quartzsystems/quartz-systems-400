     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DC3               S              3A
     DC5               S              5A
     DC8               S              8A   INZ('12345678')
     C                   EVAL      C5 = 'abcde'
     C                   MOVEL     'XY'          C5
     C                   EVAL      LINE = 'a [' + C5 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C5 = 'abcde'
     C                   MOVEL(P)  'XY'          C5
     C                   EVAL      LINE = 'b [' + C5 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C3 = 'abc'
     C                   MOVEL     C8            C3
     C                   EVAL      LINE = 'c [' + C3 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C3 = 'abc'
     C                   MOVEL(P)  C8            C3
     C                   EVAL      LINE = 'd [' + C3 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   MOVEL     C5            C8
     C                   EVAL      LINE = 'e [' + C8 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   MOVEL(P)  C3            C8
     C                   EVAL      LINE = 'f [' + C8 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C5 = 'abcde'
     C                   MOVEL     'LONGERTEXT'  C5
     C                   EVAL      LINE = 'g [' + C5 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
