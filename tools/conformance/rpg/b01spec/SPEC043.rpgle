     HDFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DG1               S              5G
     DU1               S              5C
     DV4               S             10A   VARYING(4)
     DV2               S             10A   VARYING
     DIND              S               N
     DPT               S               *
     DPP               S               *   PROCPTR
     DDT               S               D   DATFMT(*EUR)
     DTM               S               T   TIMFMT(*USA)
     DTS               S               Z
     C                   EVAL      LINE = %CHAR(%SIZE(G1)) + ' ' +
     C                             %CHAR(%LEN(G1)) + ' ' + %CHAR(%SIZE(U1)) +
     C                             ' ' + %CHAR(%LEN(U1))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      LINE = %CHAR(%SIZE(V4)) + ' ' +
     C                             %CHAR(%SIZE(V2)) + ' ' + %CHAR(%SIZE(IND)) +
     C                             ' ' + %CHAR(%SIZE(PT))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      LINE = %CHAR(%SIZE(PP)) + ' ' +
     C                             %CHAR(%SIZE(DT)) + ' ' + %CHAR(%SIZE(TM)) +
     C                             ' ' + %CHAR(%SIZE(TS))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
