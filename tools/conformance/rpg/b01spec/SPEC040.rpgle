     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DLONGK            C                   'This constant is long and +
     D                                     goes on to a second line'
     DPI               C                   3.14159
     DNEG              C                   CONST(-12)
     DHEXK             C                   X'C1C2C3'
     DQUOT             C                   'it''s'
     DN2               S              9  5 INZ(PI)
     C                   EVAL      LINE = LONGK
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      LINE = %CHAR(N2) + ' ' + %CHAR(NEG) + ' ' +
     C                             HEXK + ' ' + QUOT
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      LINE = %CHAR(%LEN(LONGK)) + ' ' +
     C                             %CHAR(%LEN(QUOT))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
