     HDFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DCOUNTER          PR            10I 0
     DI                S              3  0
     C     1             DO        3             I
     C                   EVAL      LINE = %CHAR(COUNTER())
     C                   WRITE     QSYSPRT       LINE
     C                   ENDDO
     C                   EVAL      *INLR = *ON
     PCOUNTER          B
     D                 PI            10I 0
     DN                S             10I 0 INZ(100)
     D                                     STATIC
     DM                S             10I 0 INZ(100)
     C                   EVAL      N = N + 1
     C                   EVAL      M = M + 1
     C                   RETURN    N * 1000 + M
     PCOUNTER          E
