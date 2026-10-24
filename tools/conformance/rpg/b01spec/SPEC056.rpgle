     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DA                S              5  0 INZ(4)
     DB                S              5  0 INZ(7)
     DT                S             30    VARYING
     C                   IF        A < B AND
     C                             (B - A = 3 OR
     C                             A = 0)
     C                   EVAL      T = 'it is +
     C                                 true'
     C                   ELSE
     C                   EVAL      T = 'false'
     C                   ENDIF
     C                   EVAL      LINE = T
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
