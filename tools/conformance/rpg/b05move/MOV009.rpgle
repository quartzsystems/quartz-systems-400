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
     C                   MOVE      Z5            Z3
     C                   EVAL      LINE = 'a ' + %CHAR(Z3) + ' ' +
     C                             %EDITC(Z3:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      Z3 = 999
     C                   MOVE      ZN            Z3
     C                   EVAL      LINE = 'b ' + %CHAR(Z3) + ' ' +
     C                             %EDITC(Z3:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      Z7 = 9999999
     C                   MOVE      Z5            Z7
     C                   EVAL      LINE = 'c ' + %CHAR(Z7) + ' ' +
     C                             %EDITC(Z7:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      Z7 = 9999999
     C                   MOVE      ZN            Z7
     C                   EVAL      LINE = 'd ' + %CHAR(Z7) + ' ' +
     C                             %EDITC(Z7:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      Z7 = 9999999
     C                   MOVE(P)   ZN            Z7
     C                   EVAL      LINE = 'e ' + %CHAR(Z7) + ' ' +
     C                             %EDITC(Z7:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      Z72 = -99999.99
     C                   MOVE      Z5            Z72
     C                   EVAL      LINE = 'f ' + %CHAR(Z72) + ' ' +
     C                             %EDITC(Z72:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      Z72 = 11111.11
     C                   MOVE      Z42           Z72
     C                   EVAL      LINE = 'g ' + %CHAR(Z72) + ' ' +
     C                             %EDITC(Z72:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      Z5 = 55555
     C                   MOVE      Z42           Z5
     C                   EVAL      LINE = 'h ' + %CHAR(Z5) + ' ' +
     C                             %EDITC(Z5:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      P9 = -123456.789
     C                   MOVE      Z42           P9
     C                   EVAL      LINE = 'i ' + %CHAR(P9) + ' ' +
     C                             %EDITC(P9:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   MOVE      Z7            Z7
     C                   EVAL      LINE = 'j ' + %CHAR(Z7) + ' ' +
     C                             %EDITC(Z7:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
