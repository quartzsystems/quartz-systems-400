     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DR                S              5  2
     DS                S              5  0
     C                   Eval(H)   R = 2 / 3                                    rounded
     C                   z-add(h)  7.5           S                              rounded too
     C                   EVAL      LINE = %CHAR(R) + ' ' + %CHAR(S)
     C                   WRITE     QSYSPRT       LINE
     C                   eval      R = 2 / 3
     C                   Z-ADD     7.5           S
     C                   EVAL      LINE = %CHAR(R) + ' ' + %CHAR(S)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
