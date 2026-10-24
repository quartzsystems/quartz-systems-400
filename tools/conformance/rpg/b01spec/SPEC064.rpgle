     HDFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DCAT3             PR            30    VARYING
     D                               10    VARYING CONST
     D                               10    VARYING CONST
     D                               10    VARYING CONST
     DSHOWIT           PR
     D                               40    CONST
     C                   EVAL      LINE = CAT3('alpha' :
     C                             'beta' :
     C                             'gamma')
     C                   WRITE     QSYSPRT       LINE
     C                   CALLP     SHOWIT('called ' +
     C                             'with continuation')
     C                   EVAL      *INLR = *ON
     PCAT3             B
     D                 PI            30    VARYING
     DA                              10    VARYING CONST
     DB                              10    VARYING CONST
     DC                              10    VARYING CONST
     C                   RETURN    A + '-' + B + '-' + C
     PCAT3             E
     PSHOWIT           B
     D                 PI
     DT                              40    CONST
     C                   EVAL      LINE = T
     C                   WRITE     QSYSPRT       LINE
     PSHOWIT           E
