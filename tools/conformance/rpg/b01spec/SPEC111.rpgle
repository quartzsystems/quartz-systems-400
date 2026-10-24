     HDFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DMIX              PR
     C                   CALLP     MIX
     C                   EVAL      *INLR = *ON
     PMIX              B
     DN                S              5  0 INZ(1)
       n += 1;
     C                   ADD       10            N
       line = 'mixed ' + %char(n);
     C                   WRITE     QSYSPRT       LINE
     PMIX              E
