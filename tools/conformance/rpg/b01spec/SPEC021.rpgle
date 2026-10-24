     HDFTACTGRP(*NO)
     HDATFMT(*USA) TIMFMT(*HMS)
     HDECEDIT('0.')
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DDT               S               D   INZ(D'03/05/2026')
     DTM               S               T   INZ(T'14:30:05')
     DN                S              5  2 INZ(.25)
     C                   EVAL      LINE = %CHAR(DT) + ' ' + %CHAR(TM) + ' ' +
     C                             %CHAR(N)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      LINE = %CHAR(DT:*ISO) + ' ' + %CHAR(TM:*ISO)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
