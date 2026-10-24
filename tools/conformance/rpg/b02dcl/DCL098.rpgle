     H DFTACTGRP(*NO) DATFMT(*EUR) TIMFMT(*JIS)
     FQSYSPRT   O    F  132        PRINTER
     D line            DS           132
     D D1              S               D   INZ(D'05.03.2026')
     D D2              S               D   DATFMT(*YMD/) INZ(D'31.12.2039')
     D D3              S               D   DATFMT(*JUL) INZ(D'01.01.1940')
     D T1              S               T   INZ(T'14:30:05')
     D T2              S               T   TIMFMT(*HMS,) INZ(T'00:00:00')
      /free
        line = %char(D1) + ' ' + %char(D2) + ' ' + %char(D3);
        write QSYSPRT line;
        line = %char(T1) + ' ' + %char(T2) + ' ' + %char(%size(D3));
        write QSYSPRT line;
        *inlr = *on;
      /end-free
