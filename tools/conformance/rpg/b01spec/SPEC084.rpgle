     HDFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DCALC             PR            10I 0
     C                   EVAL      LINE = %CHAR(CALC())
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
     PCALC             B
     D                 PI            10I 0
     DV                S             10I 0
     C                   EXSR      INIT
     C                   RETURN    V
     C     INIT          BEGSR
     C                   EVAL      V = 21 * 2
     C                   ENDSR
     PCALC             E
