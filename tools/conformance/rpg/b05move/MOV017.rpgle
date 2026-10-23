     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DA2               S              2A   DIM(4)
     DC5               S              5A
     DC10              S             10A
     DI                S              3P 0
     C                   EVAL      A2 = '..'
     C                   MOVEA     'abcde'       A2
     C                   EVAL      LINE = 'a ' + A2(1) + A2(2) + A2(3) + A2(4)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      A2 = '..'
     C                   MOVEA(P)  'abc'         A2
     C                   EVAL      LINE = 'b [' + A2(1) + A2(2) + A2(3) + A2(4)
     C                             + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      A2 = '..'
     C                   EVAL      I = 2
     C                   MOVEA     'xyz'         A2(I)
     C                   EVAL      LINE = 'c ' + A2(1) + A2(2) + A2(3) + A2(4)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      A2(1) = 'ab'
     C                   EVAL      A2(2) = 'cd'
     C                   EVAL      A2(3) = 'ef'
     C                   EVAL      A2(4) = 'gh'
     C                   EVAL      C5 = '*****'
     C                   MOVEA     A2            C5
     C                   EVAL      LINE = 'd [' + C5 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C10 = '**********'
     C                   MOVEA     A2(3)         C10
     C                   EVAL      LINE = 'e [' + C10 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C10 = '**********'
     C                   MOVEA(P)  A2(3)         C10
     C                   EVAL      LINE = 'f [' + C10 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C5 = 'vwxyz'
     C                   EVAL      A2 = '..'
     C                   MOVEA     C5            A2(2)
     C                   EVAL      LINE = 'g ' + A2(1) + A2(2) + A2(3) + A2(4)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      A2 = '..'
     C                   MOVEA     *ALL'q'       A2
     C                   EVAL      LINE = 'h ' + A2(1) + A2(2) + A2(3) + A2(4)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
