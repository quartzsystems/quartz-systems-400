     HDFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER PRTCTL(PC)
     DPC               DS
     D SPB                     1      3
     D SPA                     4      6
     D SKB                     7      9
     D SKA                    10     12
     D CUR                    13     15S 0
     DTXT              S             20
     DSAVE             S              3S 0
     C                   EVAL      SPB = '  1'
     C                   EVAL      TXT = 'first'
     C                   EXCEPT    ONE
     C                   EVAL      SAVE = CUR
     C                   EVAL      SPB = '  2'
     C                   EVAL      TXT = 'after two'
     C                   EXCEPT    ONE
     C                   EVAL      SPB = ' '
     C                   EVAL      SKB = ' 10'
     C                   EVAL      TXT = 'at ten ' + %CHAR(SAVE) + ' ' +
     C                             %CHAR(CUR)
     C                   EXCEPT    ONE
     C                   EVAL      *INLR = *ON
     OQSYSPRT   E            ONE
     O                       TXT                 20
