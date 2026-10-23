     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DARR              S              3A   DIM(5)
     DNUM              S              5P 0 DIM(4)
     DI                S              3P 0
     DJ                S              5I 0
     DK5               S              5P 0
     C                   EVAL      ARR(1) = 'aaa'
     C                   EVAL      ARR(2) = 'bbb'
     C                   EVAL      ARR(3) = 'ccc'
     C                   EVAL      ARR(4) = 'bbb'
     C                   EVAL      ARR(5) = 'eee'
     C                   EVAL      NUM(1) = 7
     C                   EVAL      NUM(2) = -3
     C                   EVAL      NUM(3) = 12
     C                   EVAL      NUM(4) = -3
     C                   EVAL      I = 1
     C     'ccc'         LOOKUP    ARR(I)                                 50
     C                   EVAL      LINE = 'a ' + %CHAR(I) + ' ' + *IN50
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      I = 1
     C     'zzz'         LOOKUP    ARR(I)                                 50
     C                   EVAL      LINE = 'b ' + %CHAR(I) + ' ' + *IN50
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      I = 3
     C     'bbb'         LOOKUP    ARR(I)                                 50
     C                   EVAL      LINE = 'c ' + %CHAR(I) + ' ' + *IN50
     C                   WRITE     QSYSPRT       LINE
     C     'eee'         LOOKUP    ARR                                    51
     C                   EVAL      LINE = 'd ' + *IN51
     C                   WRITE     QSYSPRT       LINE
     C     'xxx'         LOOKUP    ARR                                    51
     C                   EVAL      LINE = 'e ' + *IN51
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      K5 = -3
     C                   EVAL      J = 1
     C     K5            LOOKUP    NUM(J)                                 52
     C                   EVAL      LINE = 'f ' + %CHAR(J) + ' ' + *IN52
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      J = 3
     C     K5            LOOKUP    NUM(J)                                 52
     C                   EVAL      LINE = 'g ' + %CHAR(J) + ' ' + *IN52
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      I = 4
     C     'aaa'         LOOKUP    ARR(I)                                 50
     C                   EVAL      LINE = 'h ' + %CHAR(I) + ' ' + *IN50
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      I = 2
     C     'b  '         LOOKUP    ARR(I)                                 50
     C                   EVAL      LINE = 'i ' + %CHAR(I) + ' ' + *IN50
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      K5 = 12
     C     K5            LOOKUP    NUM(3)                                 53
     C                   EVAL      LINE = 'j ' + *IN53
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
