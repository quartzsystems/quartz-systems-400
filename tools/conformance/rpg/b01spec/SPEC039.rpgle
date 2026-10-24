     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DT1               S             60    VARYING INZ('Hello, +
     D                                        world')
     DT2               S             60    VARYING INZ('ab-
     D                                        cd')
     DT3               S             60    VARYING INZ('one +
     D                                           two +
     D                                     three')
     C                   EVAL      LINE = '[' + T1 + '] ' + %CHAR(%LEN(T1))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      LINE = '[' + T2 + '] ' + %CHAR(%LEN(T2))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      LINE = '[' + T3 + '] ' + %CHAR(%LEN(T3))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
