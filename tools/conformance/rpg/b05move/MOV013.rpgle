     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DV8               S              8A   VARYING
     DV3               S              3A   VARYING
     DC5               S              5A
     DZ5               S              5S 0
     C                   EVAL      V8 = 'abc'
     C                   MOVE      'XY'          V8
     C                   EVAL      LINE = 'a [' + V8 + '] ' + %CHAR(%LEN(V8))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      V8 = 'abcdef'
     C                   MOVEL     'XY'          V8
     C                   EVAL      LINE = 'b [' + V8 + '] ' + %CHAR(%LEN(V8))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      V8 = 'abcdef'
     C                   MOVE(P)   'XY'          V8
     C                   EVAL      LINE = 'c [' + V8 + '] ' + %CHAR(%LEN(V8))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      V8 = 'abcdef'
     C                   MOVEL(P)  'XY'          V8
     C                   EVAL      LINE = 'd [' + V8 + '] ' + %CHAR(%LEN(V8))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      V3 = 'pq'
     C                   EVAL      C5 = 'abcde'
     C                   MOVE      V3            C5
     C                   EVAL      LINE = 'e [' + C5 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      V3 = 'pq'
     C                   EVAL      C5 = 'abcde'
     C                   MOVEL     V3            C5
     C                   EVAL      LINE = 'f [' + C5 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      V3 = ''
     C                   EVAL      C5 = 'abcde'
     C                   MOVE      V3            C5
     C                   EVAL      LINE = 'g [' + C5 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      V3 = '12'
     C                   EVAL      Z5 = 55555
     C                   MOVE      V3            Z5
     C                   EVAL      LINE = 'h ' + %CHAR(Z5) + ' ' +
     C                             %EDITC(Z5:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      V8 = 'abcd'
     C                   MOVE      Z5            V8
     C                   EVAL      LINE = 'i [' + V8 + '] ' + %CHAR(%LEN(V8))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      V8 = ''
     C                   MOVE      'XY'          V8
     C                   EVAL      LINE = 'j [' + V8 + '] ' + %CHAR(%LEN(V8))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
