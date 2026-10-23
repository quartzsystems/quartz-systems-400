     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DZ4               S              4S 0
     DP4               S              4P 0
     C                   EVAL      Z4 = 0
     C                   MOVE      'abcd'        Z4
     C                   EVAL      LINE = 'b ' + %CHAR(Z4) + ' ' +
     C                             %EDITC(Z4:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      Z4 = 0
     C                   MOVE      '{}AJ'        Z4
     C                   EVAL      LINE = 'c ' + %CHAR(Z4) + ' ' +
     C                             %EDITC(Z4:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      Z4 = 0
     C                   MOVE      '12 }'        Z4
     C                   EVAL      LINE = 'd ' + %CHAR(Z4) + ' ' +
     C                             %EDITC(Z4:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      Z4 = 0
     C                   MOVE      'STUV'        Z4
     C                   EVAL      LINE = 'e ' + %CHAR(Z4) + ' ' +
     C                             %EDITC(Z4:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      P4 = 0
     C                   MOVE      '12 }'        P4
     C                   EVAL      LINE = 'g ' + %CHAR(P4) + ' ' +
     C                             %EDITC(P4:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      P4 = 0
     C                   MOVE      'STUV'        P4
     C                   EVAL      LINE = 'h ' + %CHAR(P4) + ' ' +
     C                             %EDITC(P4:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      P4 = -1234
     C                   MOVEL     '9'           P4
     C                   EVAL      LINE = 'i ' + %CHAR(P4) + ' ' +
     C                             %EDITC(P4:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
