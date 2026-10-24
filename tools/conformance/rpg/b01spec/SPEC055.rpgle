     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DS                S             20    INZ('hello world')
     C     'o'           SCAN      S             P1                3 0
     C     5             SUBST     S:3           R1                5
     C     'lo':'LO'     XLATE     S             R2               20
     C     'helo '       CHECK     S             P2                3 0
     C     ' '           CHECKR    S             P3                3 0
     C                   EVAL      LINE = %CHAR(P1) + ' [' + R1 + '] ' +
     C                             %TRIM(R2) + ' ' + %CHAR(P2) + ' ' + %CHAR(P3)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
