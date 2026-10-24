       dcl-f QSYSPRT printer(132) oflind(*inof);
     DTOT              S              7  2 INZ(12.5)
     C                   EXCEPT    LN
     C                   EVAL      *INLR = *ON
     OQSYSPRT   E            LN             1
     O                                           10 'total'
     O                       TOT           2     25
