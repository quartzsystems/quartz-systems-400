     FQSYSPRT   O    F  132        PRINTER
     DN                S              5  0 INZ(77)
     DS                S              5    INZ('abc')
     C                   EXCEPT    LN
     C                   EXCEPT    LN
     C                   EVAL      *INLR = *ON
     OQSYSPRT   E            LN             1
     O                       S              B    10
     O                       N             1B    +2
     O                                           +1 'x'
