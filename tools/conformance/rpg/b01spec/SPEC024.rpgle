     hdftactgrp(*no)                                                            first control line
     HOPTION(*XREF:
     H       *NODEBUGIO)
     HDECEDIT('0,')
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DN                S              5  2 INZ(.5)
     C                   EVAL      LINE = %CHAR(N) + ' ' + %CHAR(N * -3)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
