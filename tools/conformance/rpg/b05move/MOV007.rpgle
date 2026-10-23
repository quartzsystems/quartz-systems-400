     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DZ5               S              5S 0
     DZ3               S              3S 0
     DZ72              S              7S 2
     DC3               S              3A   INZ('123')
     DC5               S              5A   INZ('9876J')
     DCN               S              3A   INZ('12L')
     DC8               S              8A   INZ('1234567R')
     C                   EVAL      Z5 = 55555
     C                   MOVEL     C3            Z5
     C                   EVAL      LINE = 'a ' + %CHAR(Z5) + ' ' +
     C                             %EDITC(Z5:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      Z5 = -55555
     C                   MOVEL     C3            Z5
     C                   EVAL      LINE = 'b ' + %CHAR(Z5) + ' ' +
     C                             %EDITC(Z5:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      Z5 = -55555
     C                   MOVEL(P)  C3            Z5
     C                   EVAL      LINE = 'c ' + %CHAR(Z5) + ' ' +
     C                             %EDITC(Z5:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      Z5 = 55555
     C                   MOVEL     C5            Z5
     C                   EVAL      LINE = 'd ' + %CHAR(Z5) + ' ' +
     C                             %EDITC(Z5:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      Z5 = 55555
     C                   MOVEL     CN            Z5
     C                   EVAL      LINE = 'e ' + %CHAR(Z5) + ' ' +
     C                             %EDITC(Z5:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      Z5 = 55555
     C                   MOVEL(P)  CN            Z5
     C                   EVAL      LINE = 'f ' + %CHAR(Z5) + ' ' +
     C                             %EDITC(Z5:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      Z3 = 555
     C                   MOVEL     C8            Z3
     C                   EVAL      LINE = 'g ' + %CHAR(Z3) + ' ' +
     C                             %EDITC(Z3:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      Z72 = 11111.11
     C                   MOVEL     C8            Z72
     C                   EVAL      LINE = 'h ' + %CHAR(Z72) + ' ' +
     C                             %EDITC(Z72:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      Z3 = 555
     C                   MOVEL     CN            Z3
     C                   EVAL      LINE = 'i ' + %CHAR(Z3) + ' ' +
     C                             %EDITC(Z3:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
