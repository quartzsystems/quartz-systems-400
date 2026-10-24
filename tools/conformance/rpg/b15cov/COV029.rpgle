     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DN                S              3P 0
     DM                S              3P 0 INZ(5)
     C     1             DO        4             N
     C                   SELECT
     C     N             WHENEQ    1
     C                   EVAL      LINE = %CHAR(N) + ' one'
     C                   WRITE     QSYSPRT       LINE
     C     N             WHENEQ    2
     C     M             ANDGT     4
     C                   EVAL      LINE = %CHAR(N) + ' two and m'
     C                   WRITE     QSYSPRT       LINE
     C     N             WHENEQ    9
     C     N             OREQ      3
     C                   EVAL      LINE = %CHAR(N) + ' three'
     C                   WRITE     QSYSPRT       LINE
     C                   OTHER
     C                   EVAL      LINE = %CHAR(N) + ' other'
     C                   WRITE     QSYSPRT       LINE
     C                   ENDSL
     C                   ENDDO
     C                   EVAL      *INLR = *ON
