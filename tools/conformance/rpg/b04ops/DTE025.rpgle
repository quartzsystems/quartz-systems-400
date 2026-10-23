     HDFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DN6               S              6S 0
     DN12              S             12P 0
     DN14              S             14P 0
     DTM               S               T
     DDT               S               D
     DTS               S               Z
     C                   TIME                    N6
     C                   TIME                    N12
     C                   TIME                    N14
     C                   TIME                    TM
     C                   TIME                    DT
     C                   TIME                    TS
     C                   IF        N6 >= 0 AND N6 <= 235959 AND
     C                             %DIV(N12:1000000) = N6
     C                   EVAL      LINE = 'numeric ok'
     C                   WRITE     QSYSPRT       LINE
     C                   ENDIF
     C                   IF        %DIV(N14:100000000) = N6 AND
     C                             %ABS(%DIFF(%TIME():TM:*S)) < 60
     C                   EVAL      LINE = 'time ok'
     C                   WRITE     QSYSPRT       LINE
     C                   ENDIF
     C                   IF        %DIFF(%DATE():DT:*D) <= 1 AND
     C                             %DIFF(%TIMESTAMP():TS:*S) < 60
     C                   EVAL      LINE = 'date ok'
     C                   WRITE     QSYSPRT       LINE
     C                   ENDIF
     C                   SETON                                        LR
