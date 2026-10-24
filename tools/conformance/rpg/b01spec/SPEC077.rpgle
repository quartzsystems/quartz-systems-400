     FQSYSPRT   O    F  132        PRINTER
     C                   SETON                                        10
     C                   SETOFF                                       1112
     C                   EXCEPT    LN
     C                   EVAL      *INLR = *ON
     OQSYSPRT   E    10      LN             1
     O         AND  N12
     O                                            5 'rec'
     O               10                          10 'ten'
     O               11                          20 'eleven'
     O              N11                          30 'not11'
     OQSYSPRT   E    11      LN             1
     O                                           10 'never'
     OQSYSPRT   E   N11      LN             1
     O                                           10 'second'
