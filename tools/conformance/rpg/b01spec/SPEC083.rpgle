     HDFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     Ddbl              pr            10i 0
     D                               10i 0 value
     C                   EVAL      LINE = %char(dbl(21))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
     pdbl              b                   export
     D                 pi            10i 0
     Dn                              10i 0 value
     C                   return    n * 2
     pdbl              e
