     HDFTACTGRP(*NO) ACTGRP(*NEW)
     HDATFMT(*USA) TIMFMT(*HMS) DECEDIT('0,')
      *; run: no
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DD1               S               D   INZ(D'2026-10-23')
     C                   EVAL      LINE = %CHAR(D1)
     C                   WRITE     QSYSPRT       LINE
     C                   SETON                                        LR
