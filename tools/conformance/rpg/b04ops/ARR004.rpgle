     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DNUM              S              5P 2 DIM(4)
     DSUM              S              7P 2
     DSMALL            S              3P 1
     DIA               S             10I 0 DIM(3)
     DISUM             S             10I 0
     C                   EVAL      NUM(1) = 1.25
     C                   EVAL      NUM(2) = 2.50
     C                   EVAL      NUM(3) = -0.75
     C                   EVAL      NUM(4) = 10
     C                   XFOOT     NUM           SUM                  414243
     C                   EVAL      LINE = 'a ' + %CHAR(SUM) + ' ' + *IN41 +
     C                             *IN42 + *IN43
     C                   WRITE     QSYSPRT       LINE
     C                   XFOOT(H)  NUM           SMALL
     C                   EVAL      LINE = 'b ' + %CHAR(SMALL)
     C                   WRITE     QSYSPRT       LINE
     C                   XFOOT     NUM           SMALL
     C                   EVAL      LINE = 'c ' + %CHAR(SMALL)
     C                   WRITE     QSYSPRT       LINE
     C                   XFOOT     NUM           NUM(1)
     C                   EVAL      LINE = 'd ' + %CHAR(NUM(1))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      NUM = 0
     C                   XFOOT     NUM           SUM                  414243
     C                   EVAL      LINE = 'e ' + %CHAR(SUM) + ' ' + *IN41 +
     C                             *IN42 + *IN43
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      IA(1) = 5
     C                   EVAL      IA(2) = -12
     C                   EVAL      IA(3) = 4
     C                   XFOOT     IA            ISUM                 414243
     C                   EVAL      LINE = 'f ' + %CHAR(ISUM) + ' ' + *IN41 +
     C                             *IN42 + *IN43
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      LINE = 'g ' + %CHAR(%XFOOT(NUM)) + ' ' +
     C                             %CHAR(%XFOOT(IA))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
