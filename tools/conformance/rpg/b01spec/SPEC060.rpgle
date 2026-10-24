     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DI                S              3  0
     C     1             DO        9             I
     C                   EVAL      LINE = 'i=' + %CHAR(I)
     C                   WRITE     QSYSPRT       LINE
     C                   ENDDO     3
     C                   EVAL      LINE = 'after ' + %CHAR(I)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
