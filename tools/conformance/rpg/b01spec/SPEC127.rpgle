     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DA                S              5  0 INZ(10)
     DB                S              5  0 INZ(0)
     DR                S              5  0
     C                   MONITOR
     C                   EVAL      R = A / B
     C                   EVAL      LINE = 'no error'
     C                   WRITE     QSYSPRT       LINE
     C                   ON-ERROR  00102
     C                   EVAL      LINE = 'divide by zero'
     C                   WRITE     QSYSPRT       LINE
     C                   ON-ERROR
     C                   EVAL      LINE = 'other'
     C                   WRITE     QSYSPRT       LINE
     C                   ENDMON
     C                   EVAL      LINE = 'status ' + %CHAR(%STATUS)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
