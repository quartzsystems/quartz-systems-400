     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DP5               S              5P 0
     DP72              S              7P 2
     DB4               S              4B 0
     DI10              S             10I 0
     DU5               S              5U 0
     DC3               S              3A   INZ('123')
     DCN               S              3A   INZ('12L')
     DC8               S              8A   INZ('87654321')
     C                   EVAL      P5 = 55555
     C                   MOVE      C3            P5
     C                   EVAL      LINE = 'a ' + %CHAR(P5) + ' ' +
     C                             %EDITC(P5:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      P5 = 55555
     C                   MOVE      CN            P5
     C                   EVAL      LINE = 'b ' + %CHAR(P5) + ' ' +
     C                             %EDITC(P5:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      P5 = -55555
     C                   MOVEL     C3            P5
     C                   EVAL      LINE = 'c ' + %CHAR(P5) + ' ' +
     C                             %EDITC(P5:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      P72 = 0
     C                   MOVE      C8            P72
     C                   EVAL      LINE = 'd ' + %CHAR(P72) + ' ' +
     C                             %EDITC(P72:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      B4 = 5555
     C                   MOVE      CN            B4
     C                   EVAL      LINE = 'e ' + %CHAR(B4) + ' ' +
     C                             %EDITC(B4:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      I10 = 0
     C                   MOVE      C8            I10
     C                   EVAL      LINE = 'f ' + %CHAR(I10) + ' ' +
     C                             %EDITC(I10:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      I10 = 1111111111
     C                   MOVEL     C3            I10
     C                   EVAL      LINE = 'g ' + %CHAR(I10) + ' ' +
     C                             %EDITC(I10:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      U5 = 55555
     C                   MOVE      C3            U5
     C                   EVAL      LINE = 'h ' + %CHAR(U5) + ' ' +
     C                             %EDITC(U5:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      P72 = 11111.11
     C                   MOVE(P)   CN            P72
     C                   EVAL      LINE = 'i ' + %CHAR(P72) + ' ' +
     C                             %EDITC(P72:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
