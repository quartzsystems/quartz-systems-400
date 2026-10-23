     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DZ5               S              5S 0 INZ(12345)
     DZN               S              5S 0 INZ(-12345)
     DZ3               S              3S 0
     DZ7               S              7S 0
     DZ72              S              7S 2
     DZ42              S              4S 2 INZ(-98.76)
     DP9               S              9P 3
     C                   EVAL      Z3 = 999
     C                   MOVEL     Z5            Z3
     C                   EVAL      LINE = 'a ' + %CHAR(Z3) + ' ' +
     C                             %EDITC(Z3:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      Z3 = 999
     C                   MOVEL     ZN            Z3
     C                   EVAL      LINE = 'b ' + %CHAR(Z3) + ' ' +
     C                             %EDITC(Z3:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      Z7 = 9999999
     C                   MOVEL     Z5            Z7
     C                   EVAL      LINE = 'c ' + %CHAR(Z7) + ' ' +
     C                             %EDITC(Z7:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      Z7 = 9999999
     C                   MOVEL     ZN            Z7
     C                   EVAL      LINE = 'd ' + %CHAR(Z7) + ' ' +
     C                             %EDITC(Z7:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      Z7 = -9999999
     C                   MOVEL     Z5            Z7
     C                   EVAL      LINE = 'e ' + %CHAR(Z7) + ' ' +
     C                             %EDITC(Z7:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      Z7 = 9999999
     C                   MOVEL(P)  ZN            Z7
     C                   EVAL      LINE = 'f ' + %CHAR(Z7) + ' ' +
     C                             %EDITC(Z7:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      Z7 = -9999999
     C                   MOVEL(P)  Z5            Z7
     C                   EVAL      LINE = 'g ' + %CHAR(Z7) + ' ' +
     C                             %EDITC(Z7:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      Z72 = 11111.11
     C                   MOVEL     Z42           Z72
     C                   EVAL      LINE = 'h ' + %CHAR(Z72) + ' ' +
     C                             %EDITC(Z72:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      P9 = -123456.789
     C                   MOVEL     Z42           P9
     C                   EVAL      LINE = 'i ' + %CHAR(P9) + ' ' +
     C                             %EDITC(P9:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      Z42 = 11.11
     C                   MOVEL     ZN            Z42
     C                   EVAL      LINE = 'j ' + %CHAR(Z42) + ' ' +
     C                             %EDITC(Z42:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
