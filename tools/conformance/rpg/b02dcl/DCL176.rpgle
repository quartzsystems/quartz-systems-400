     H DFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     D line            DS           132
     DV0               S             10S 1 INZ(-1069.6)
     DV1               S               N   DIM(3)
     DV2               S              4F   INZ(*LOVAL)
     DV3               S             20I 0 DIM(3) INZ(*LOVAL)
     DV4               S              4C   INZ(U'0075')
     DV5               S               D   DATFMT(*JUL.) INZ(D'2026-03-05')
     DV6               S               Z   INZ(Z'2026-03-05-14.30.05.000123')
     DV7               S              5C
      /free
        line = 'V0 ' + %char(%size(V0)) + ' ' + %char(%len(V0)) + ' ' +
        %char(V0);
        write QSYSPRT line;
        line = 'V1 ' + %char(%size(V1(3))) + ' ' + V1(3);
        write QSYSPRT line;
        line = 'V2 ' + %char(%size(V2)) + ' ' + %char(V2);
        write QSYSPRT line;
        line = 'V3 ' + %char(%size(V3(3))) + ' ' + %char(%len(V3(3))) + ' ' +
        %char(V3(3));
        write QSYSPRT line;
        line = 'V4 ' + %char(%size(V4)) + ' ' + %char(%len(V4)) + ' ' +
        %char(V4);
        write QSYSPRT line;
        line = 'V5 ' + %char(%size(V5)) + ' ' + %char(V5);
        write QSYSPRT line;
        line = 'V6 ' + %char(%size(V6)) + ' ' + %char(V6);
        write QSYSPRT line;
        line = 'V7 ' + %char(%size(V7)) + ' ' + %char(%len(V7)) + ' ' +
        %char(V7);
        write QSYSPRT line;
        *inlr = *on;
      /end-free
