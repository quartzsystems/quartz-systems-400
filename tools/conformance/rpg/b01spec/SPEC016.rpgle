     HDFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DTWICE            PR             9P 2
     D                                7P 2 VALUE
     C                   EVAL      LINE = 'twice=' + %CHAR(TWICE(4.25))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
     PTWICE            B
     D                 PI             9P 2
     DN                               7P 2 VALUE
     C                   RETURN    N * 2
     PTWICE            E
