     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DASC              S              3P 0 DIM(5) ASCEND
     DDSC              S              3P 0 DIM(5) DESCEND
     DI                S              3P 0
     DCA               S              2A   DIM(4) ASCEND
     DK                S              3P 0
     C                   EVAL      ASC(1) = 10
     C                   EVAL      ASC(2) = 20
     C                   EVAL      ASC(3) = 30
     C                   EVAL      ASC(4) = 40
     C                   EVAL      ASC(5) = 50
     C                   EVAL      DSC(1) = 50
     C                   EVAL      DSC(2) = 40
     C                   EVAL      DSC(3) = 30
     C                   EVAL      DSC(4) = 20
     C                   EVAL      DSC(5) = 10
     C                   EVAL      CA(1) = 'b'
     C                   EVAL      CA(2) = 'd'
     C                   EVAL      CA(3) = 'f'
     C                   EVAL      CA(4) = 'h'
     C                   EVAL      I = 1
     C                   EVAL      K = 25
     C     K             LOOKUP    ASC(I)                             51
     C                   EVAL      LINE = 'a ' + %CHAR(I) + ' ' + *IN51
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      I = 1
     C                   EVAL      K = 25
     C     K             LOOKUP    ASC(I)                               52
     C                   EVAL      LINE = 'b ' + %CHAR(I) + ' ' + *IN52
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      I = 1
     C                   EVAL      K = 30
     C     K             LOOKUP    ASC(I)                             51  53
     C                   EVAL      LINE = 'c ' + %CHAR(I) + ' ' + *IN51 + *IN53
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      I = 1
     C                   EVAL      K = 30
     C     K             LOOKUP    ASC(I)                               5253
     C                   EVAL      LINE = 'd ' + %CHAR(I) + ' ' + *IN52 + *IN53
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      I = 1
     C                   EVAL      K = 60
     C     K             LOOKUP    ASC(I)                             51
     C                   EVAL      LINE = 'e ' + %CHAR(I) + ' ' + *IN51
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      I = 1
     C                   EVAL      K = 5
     C     K             LOOKUP    ASC(I)                               52
     C                   EVAL      LINE = 'f ' + %CHAR(I) + ' ' + *IN52
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      I = 1
     C                   EVAL      K = 25
     C     K             LOOKUP    DSC(I)                             51
     C                   EVAL      LINE = 'g ' + %CHAR(I) + ' ' + *IN51
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      I = 1
     C                   EVAL      K = 25
     C     K             LOOKUP    DSC(I)                               52
     C                   EVAL      LINE = 'h ' + %CHAR(I) + ' ' + *IN52
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      I = 1
     C     'e '          LOOKUP    CA(I)                              51
     C                   EVAL      LINE = 'i ' + %CHAR(I) + ' ' + *IN51
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      I = 1
     C     'e '          LOOKUP    CA(I)                                52
     C                   EVAL      LINE = 'j ' + %CHAR(I) + ' ' + *IN52
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      I = 3
     C                   EVAL      K = 25
     C     K             LOOKUP    ASC(I)                               52
     C                   EVAL      LINE = 'k ' + %CHAR(I) + ' ' + *IN52
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      I = 1
     C                   EVAL      K = 40
     C     K             LOOKUP    ASC(I)                                 53
     C                   EVAL      LINE = 'l ' + %CHAR(I) + ' ' + *IN53
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
