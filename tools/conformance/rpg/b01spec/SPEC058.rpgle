     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DN                S              3  0
     C     1             DO        4             N
     C                   SELECT
     C                   WHEN      N = 1
     C                   EVAL      LINE = 'one'
     C                   WRITE     QSYSPRT       LINE
     C     N             WHENEQ    2
     C                   EVAL      LINE = 'two'
     C                   WRITE     QSYSPRT       LINE
     C     N             WHENGT    2
     C     N             ANDLT     4
     C                   EVAL      LINE = 'three'
     C                   WRITE     QSYSPRT       LINE
     C                   OTHER
     C                   EVAL      LINE = 'other ' + %CHAR(N)
     C                   WRITE     QSYSPRT       LINE
     C                   ENDSL
     C                   ENDDO
     C                   EVAL      *INLR = *ON
