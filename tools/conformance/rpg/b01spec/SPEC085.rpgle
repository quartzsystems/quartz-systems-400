     HDFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DTRIPLE           PR            10I 0
     D                               10I 0 VALUE
     C                   EVAL      LINE = %CHAR(TRIPLE(14))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
     PTRIPLE           B
       dcl-pi *n int(10);
         n int(10) value;
       end-pi;
       dcl-s t int(10);
       t = n * 3;
       return t;
     PTRIPLE           E
