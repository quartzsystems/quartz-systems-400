     HDFTACTGRP(*NO) TRUNCNBR(*YES)
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DN                S              3P 0 INZ(999)
     C                   MONITOR
     C                   ADD       1             N
     C                   EVAL      LINE = 'add ' + %CHAR(N)
     C                   WRITE     QSYSPRT       LINE
     C                   ON-ERROR
     C                   EVAL      LINE = 'add error ' + %CHAR(%STATUS) + ' ' +
     C                             %CHAR(N)
     C                   WRITE     QSYSPRT       LINE
     C                   ENDMON
     C                   MONITOR
     C                   Z-ADD     1234          N
     C                   EVAL      LINE = 'z-add ' + %CHAR(N)
     C                   WRITE     QSYSPRT       LINE
     C                   ON-ERROR
     C                   EVAL      LINE = 'z-add error ' + %CHAR(%STATUS) + ' '
     C                             + %CHAR(N)
     C                   WRITE     QSYSPRT       LINE
     C                   ENDMON
     C                   EVAL      *INLR = *ON
