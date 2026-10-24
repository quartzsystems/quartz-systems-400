     HDFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DADD3             PR            10I 0
     D                               10I 0 VALUE
     D                               10I 0 VALUE
     D                               10I 0 VALUE
     D                                     OPTIONS(*NOPASS)
     C                   EVAL      LINE = %CHAR(ADD3(1:2)) + ' ' +
     C                             %CHAR(ADD3(1:2:3))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
     PADD3             B
     D                 PI            10I 0
     DA                              10I 0 VALUE
     DB                              10I 0 VALUE
     DC                              10I 0 VALUE
     D                                     OPTIONS(*NOPASS)
     C                   IF        %PARMS < 3
     C                   RETURN    A + B
     C                   ENDIF
     C                   RETURN    A + B + C
     PADD3             E
