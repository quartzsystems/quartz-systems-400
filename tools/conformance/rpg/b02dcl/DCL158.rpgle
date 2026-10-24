     H DFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     D line            DS           132
     D BUF             S             10    INZ('pointed at')
     D DS1             DS                  QUALIFIED INZ
     D  FLAG                           N   INZ(*ON)
     D  PTR                            *   INZ(%ADDR(BUF))
     D  DT                             D   DATFMT(*MDY/)
     D  TM                             T   TIMFMT(*USA)
     D  TS                             Z   INZ(Z'2026-01-01-00.00.00.000000')
     D P2              S               *
     D OVER            S             10    BASED(P2)
      /free
        P2 = DS1.PTR;
        line = DS1.FLAG + ' ' + OVER + ' ' + %char(DS1.DT) + ' ' + %char(DS1.TM)
        + ' ' +
        %char(DS1.TS);
        write QSYSPRT line;
        line = %char(%size(DS1)) + ' ' + %char(%addr(DS1.PTR) - %addr(DS1));
        write QSYSPRT line;
        *inlr = *on;
      /end-free
