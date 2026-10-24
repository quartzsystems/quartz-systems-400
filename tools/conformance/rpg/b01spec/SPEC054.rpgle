     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     C                   Z-ADD     12.5          X                 5 2
     C     'abc'         CAT       'def':1       Y                10
     C                   MOVEL     'xyz'         Z                 6
     C     X             MULT      2             W                 7 1
     C                   EVAL      LINE = %CHAR(X) + ' [' + Y + '] [' + Z + '] '
     C                             + %CHAR(W) + ' ' + %CHAR(%SIZE(X))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
