     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DT                S              4
     DT2               S             30    VARYING
     C                   EVAL      T = X'C1C2+
     C                             C3C4'
     C                   EVAL      T2 = 'it''s +
     C                             Bob''s'
     C                   EVAL      LINE = T + ' ' + T2
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
