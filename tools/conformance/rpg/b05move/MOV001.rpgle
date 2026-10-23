     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DC3               S              3A
     DC5               S              5A
     DC8               S              8A   INZ('12345678')
     C                   EVAL      C5 = 'abcde'
     C                   MOVE      'XY'          C5
     C                   EVAL      LINE = 'a [' + C5 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C5 = 'abcde'
     C                   MOVE(P)   'XY'          C5
     C                   EVAL      LINE = 'b [' + C5 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C3 = 'abc'
     C                   MOVE      C8            C3
     C                   EVAL      LINE = 'c [' + C3 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C3 = 'abc'
     C                   MOVE(P)   C8            C3
     C                   EVAL      LINE = 'd [' + C3 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   MOVE      C5            C8
     C                   EVAL      LINE = 'e [' + C8 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C3 = 'abc'
     C                   MOVE      'Q'           C3
     C                   EVAL      LINE = 'f [' + C3 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   MOVE      C3            C3
     C                   EVAL      LINE = 'g [' + C3 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C5 = 'abcde'
     C                   MOVE(P)   'LONGERTEXT'  C5
     C                   EVAL      LINE = 'h [' + C5 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
