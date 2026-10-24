     H DFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     D line            DS           132
     D A               S             40    VARYING
     D                                     INZ('a value continued +
     D                                     here')
     D B               S              7P 2 INZ(*HIVAL)
     D C               S              3    INZ(*ALL'-')
     D E               S               D   INZ(*LOVAL) DATFMT(*USA)
     D F               S              5I 0 INZ(-32768)
     D G               S              4    INZ(X'F1F2F3F4')
      /free
        line = '[' + A + '] ' + %char(B) + ' ' + C + ' ' + %char(E) + ' ' +
        %char(F) + ' ' + G;
        write QSYSPRT line;
        *inlr = *on;
      /end-free
