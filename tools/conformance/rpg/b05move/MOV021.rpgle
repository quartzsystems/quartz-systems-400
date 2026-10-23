     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DZ9               S              9S 4 INZ(-1234.5678)
     DZ31              S             31S 0
     DC40              S             40A
     DZ1               S              1S 0
     DP1               S              1P 0
     DC1               S              1A
     C                   EVAL      Z31 = 0
     C                   MOVE      Z9            Z31
     C                   EVAL      LINE = 'a ' + %CHAR(Z31) + ' ' +
     C                             %EDITC(Z31:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      Z31 = 0
     C                   MOVEL     Z9            Z31
     C                   EVAL      LINE = 'b ' + %CHAR(Z31) + ' ' +
     C                             %EDITC(Z31:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   MOVE(P)   Z9            C40
     C                   EVAL      LINE = 'c [' + C40 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      Z1 = 0
     C                   MOVE      Z9            Z1
     C                   EVAL      LINE = 'd ' + %CHAR(Z1) + ' ' +
     C                             %EDITC(Z1:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      Z1 = 0
     C                   MOVEL     Z9            Z1
     C                   EVAL      LINE = 'e ' + %CHAR(Z1) + ' ' +
     C                             %EDITC(Z1:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      P1 = 0
     C                   MOVE      Z9            P1
     C                   EVAL      LINE = 'f ' + %CHAR(P1) + ' ' +
     C                             %EDITC(P1:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C1 = 'x'
     C                   MOVE      Z9            C1
     C                   EVAL      LINE = 'g [' + C1 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C1 = 'x'
     C                   MOVEL     Z9            C1
     C                   EVAL      LINE = 'h [' + C1 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      P1 = 0
     C                   MOVE      'K'           P1
     C                   EVAL      LINE = 'i ' + %CHAR(P1) + ' ' +
     C                             %EDITC(P1:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
