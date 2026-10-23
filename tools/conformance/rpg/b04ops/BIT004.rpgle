     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DQ                S              5P 0
     DR                S              5P 0
     DR2               S              5P 2
     DQ1               S              5P 1
     DSQ               S              7P 3
     DSI               S              5P 0
     DF                S              8F
     C     17            DIV       5             Q
     C                   MVR                     R
     C                   EVAL      LINE = 'a ' + %CHAR(Q) + ' ' + %CHAR(R)
     C                   WRITE     QSYSPRT       LINE
     C     -17           DIV       5             Q
     C                   MVR                     R
     C                   EVAL      LINE = 'b ' + %CHAR(Q) + ' ' + %CHAR(R)
     C                   WRITE     QSYSPRT       LINE
     C     10.5          DIV       4             Q
     C                   MVR                     R2
     C                   EVAL      LINE = 'c ' + %CHAR(Q) + ' ' + %CHAR(R2)
     C                   WRITE     QSYSPRT       LINE
     C     10            DIV       3             Q1
     C                   MVR                     R2
     C                   EVAL      LINE = 'd ' + %CHAR(Q1) + ' ' + %CHAR(R2)
     C                   WRITE     QSYSPRT       LINE
     C     17            DIV       -5            Q
     C                   MVR                     R                    414243
     C                   EVAL      LINE = 'e ' + %CHAR(R) + ' ' + *IN41 + *IN42
     C                             + *IN43
     C                   WRITE     QSYSPRT       LINE
     C                   SQRT      2             SQ
     C                   EVAL      LINE = 'f ' + %CHAR(SQ)
     C                   WRITE     QSYSPRT       LINE
     C                   SQRT(H)   2             SQ
     C                   EVAL      LINE = 'g ' + %CHAR(SQ)
     C                   WRITE     QSYSPRT       LINE
     C                   SQRT      17            SI
     C                   EVAL      LINE = 'h ' + %CHAR(SI)
     C                   WRITE     QSYSPRT       LINE
     C                   SQRT(H)   17            SI
     C                   EVAL      LINE = 'i ' + %CHAR(SI)
     C                   WRITE     QSYSPRT       LINE
     C                   SQRT      144           SI
     C                   EVAL      LINE = 'j ' + %CHAR(SI)
     C                   WRITE     QSYSPRT       LINE
     C                   SQRT      2             F
     C                   EVAL      LINE = 'k ' + %CHAR(%DEC(F:15:12))
     C                   WRITE     QSYSPRT       LINE
     C                   SQRT      0.25          SQ
     C                   EVAL      LINE = 'l ' + %CHAR(SQ)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
