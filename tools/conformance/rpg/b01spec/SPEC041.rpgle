     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DN1               S              5  2 INZ(1.25)
     DC1               S              5    INZ('ab')
     DDS1              DS
     D Z1                             5  2
     D X1                      6      8  0
     D Y1                      9     12
     C                   EVAL      Z1 = 12.34
     C                   EVAL      X1 = 7
     C                   EVAL      Y1 = 'abcd'
     C                   EVAL      LINE = %CHAR(%SIZE(N1)) + ' ' +
     C                             %CHAR(%SIZE(C1)) + ' ' + %CHAR(%SIZE(Z1)) +
     C                             ' ' + %CHAR(%SIZE(DS1))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      LINE = DS1 + ' ' + %CHAR(N1) + ' [' + C1 +
     C                             ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
