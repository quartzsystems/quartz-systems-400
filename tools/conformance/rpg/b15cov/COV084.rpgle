     HDFTACTGRP(*NO) OPENOPT(*INZOFL) REQPREXP(*WARN) TRUNCNBR(*NO)
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DN                S              3P 0 INZ(999)
     C                   ADD(E)    1             N
     C                   EVAL      LINE = 'add ' + %CHAR(%STATUS) + ' ' +
     C                             %CHAR(N)
     C                   WRITE     QSYSPRT       LINE
     C                   Z-ADD(E)  1234          N
     C                   EVAL      LINE = 'z-add ' + %CHAR(%STATUS) + ' ' +
     C                             %CHAR(N)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
