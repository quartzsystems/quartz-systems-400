     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     C     17            DIV       5             Q                 3 0
     C                   MVR                     R                 3 0
     C                   SQRT      16            S                 5 2
     C     -7            DIV       2             Q2                3 0
     C                   MVR                     R2                3 0
     C                   EVAL      LINE = %CHAR(Q) + ' ' + %CHAR(R) + ' ' +
     C                             %CHAR(S) + ' ' + %CHAR(Q2) + ' ' + %CHAR(R2)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
