     HDFTACTGRP(*NO) DATEDIT(*YMD-)
     FQSYSPRT   O    F  132        PRINTER
     DD8               S              8S 0 INZ(20261225)
     DD6               S              6S 0 INZ(261225)
     C                   EXCEPT    DTL
     C                   EVAL      *INLR = *ON
     OQSYSPRT   E            DTL            1
     O                       D8            Y     12
     O                       D6            Y     24
