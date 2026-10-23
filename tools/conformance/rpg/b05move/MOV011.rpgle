     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DP5               S              5P 0 INZ(-12345)
     DP3               S              3P 1
     DP9               S              9P 0
     DB4               S              4B 0
     DI5               S              5I 0
     DI10              S             10I 0
     DU3               S              3U 0
     DI20              S             20I 0 INZ(-98765432109876)
     C                   EVAL      P3 = 99.9
     C                   MOVE      P5            P3
     C                   EVAL      LINE = 'a ' + %CHAR(P3) + ' ' +
     C                             %EDITC(P3:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      P3 = 99.9
     C                   MOVEL     P5            P3
     C                   EVAL      LINE = 'b ' + %CHAR(P3) + ' ' +
     C                             %EDITC(P3:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      B4 = 1111
     C                   MOVE      P5            B4
     C                   EVAL      LINE = 'c ' + %CHAR(B4) + ' ' +
     C                             %EDITC(B4:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      I5 = 11111
     C                   MOVE      P5            I5
     C                   EVAL      LINE = 'e ' + %CHAR(I5) + ' ' +
     C                             %EDITC(I5:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      I10 = 1111111111
     C                   MOVEL     P5            I10
     C                   EVAL      LINE = 'f ' + %CHAR(I10) + ' ' +
     C                             %EDITC(I10:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      P9 = 0
     C                   MOVE      I20           P9
     C                   EVAL      LINE = 'h ' + %CHAR(P9) + ' ' +
     C                             %EDITC(P9:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      P9 = 0
     C                   MOVEL     I20           P9
     C                   EVAL      LINE = 'i ' + %CHAR(P9) + ' ' +
     C                             %EDITC(P9:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      I10 = 1111111111
     C                   MOVE(P)   P3            I10
     C                   EVAL      LINE = 'j ' + %CHAR(I10) + ' ' +
     C                             %EDITC(I10:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
