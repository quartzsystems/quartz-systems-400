     FQSYSPRT   O    F  132        PRINTER
     DAMT              S              9  2 INZ(-1234.56)
     DAMT2             S              9  2 INZ(5)
     C                   EXCEPT    LN
     C                   EVAL      *INLR = *ON
     OQSYSPRT   E            LN             1
     O                       AMT                 30 ' ,   ,  0.  -'
     O                       AMT2                60 ' ,   ,  0.  CR'
     OQSYSPRT   E            LN             1
     O                       AMT                 30 '*   ,   .  '
     O                       AMT2                60 '   ,   0.  &CR'
