     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DC2               S              2A   INZ('AB')
     DC3               S              3A
     DZ3               S              3S 0 INZ(-123)
     DZ4               S              4S 0
     C                   EVAL      C3 = 'ABC'
     C                   MHHZO     'J9'          C3
     C                   EVAL      LINE = 'a [' + C3 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C3 = 'ABC'
     C                   MHLZO     'J9'          C3
     C                   EVAL      LINE = 'b [' + C3 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C3 = 'ABC'
     C                   MLHZO     '9J'          C3
     C                   EVAL      LINE = 'c [' + C3 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C3 = 'ABC'
     C                   MLLZO     'AJ'          C3
     C                   EVAL      LINE = 'd [' + C3 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C3 = 'ABC'
     C                   MLLZO     Z3            C3
     C                   EVAL      LINE = 'e [' + C3 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      Z4 = 1234
     C                   MLLZO     C2            Z4
     C                   EVAL      LINE = 'f ' + %CHAR(Z4) + ' ' +
     C                             %EDITC(Z4:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      C3 = 'ABC'
     C                   MLHZO     Z3            C3
     C                   EVAL      LINE = 'g [' + C3 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      Z4 = 1234
     C                   MHLZO     'J'           Z4
     C                   EVAL      LINE = 'h ' + %CHAR(Z4) + ' ' +
     C                             %EDITC(Z4:'X')
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
