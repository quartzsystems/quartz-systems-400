     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DA2               S              2A   DIM(4)
     DA3               S              3A   DIM(3)
     DA5               S              5A   DIM(2)
     DI                S              3P 0
     C                   EVAL      A3(1) = 'abc'
     C                   EVAL      A3(2) = 'def'
     C                   EVAL      A3(3) = 'ghi'
     C                   EVAL      A2 = '..'
     C                   MOVEA     A3            A2
     C                   EVAL      LINE = 'a ' + A2(1) + A2(2) + A2(3) + A2(4)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      A5 = '-----'
     C                   MOVEA     A3            A5
     C                   EVAL      LINE = 'b ' + A5(1) + A5(2)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      A2 = '..'
     C                   MOVEA     A3(2)         A2(2)
     C                   EVAL      LINE = 'c ' + A2(1) + A2(2) + A2(3) + A2(4)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      A5 = '-----'
     C                   MOVEA(P)  A3(3)         A5
     C                   EVAL      LINE = 'd [' + A5(1) + A5(2) + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      A2 = '..'
     C                   EVAL      I = 3
     C                   MOVEA     A5            A2(I)
     C                   EVAL      LINE = 'e ' + A2(1) + A2(2) + A2(3) + A2(4)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      A3 = 'zzz'
     C                   MOVEA(P)  A2(4)         A3(2)
     C                   EVAL      LINE = 'f [' + A3(1) + A3(2) + A3(3) + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
