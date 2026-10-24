     H DFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     D line            DS           132
     D ARR             S              3  0 DIM(4) CTDATA PERRCD(4)
     D ARA             S              2    DIM(3) CTDATA PERRCD(1)
     D ARB             S              4    DIM(3) ALT(ARA)
     D ARR2            S                   LIKE(ARR) DIM(2)
      /free
        line = %char(ARR(1)) + ' ' + %char(ARR(4)) + ' ' + ARA(2) + ARB(2) + ' '
        +
        %char(%elem(ARR2)) + ' ' + %char(%size(ARR2));
        write QSYSPRT line;
        *inlr = *on;
      /end-free
**CTDATA ARR
001002003004
**CTDATA ARA
X1one
X2two
X3thr
