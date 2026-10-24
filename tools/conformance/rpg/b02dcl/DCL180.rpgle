     H DFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     D line            DS           132
     DV0               S              8F   INZ(-0.25)
     DV1               S             14S 0 INZ(5533)
     DV2               S              6B 2
     DV3               S              8A   VARYING INZ(*BLANKS)
     DV4               S               Z   INZ(Z'2026-03-05-14.30.05.000123')
     DV5               S              2P 1 DIM(3)
     DV6               S              5I 0 DIM(3) INZ(*LOVAL)
     DV7               S              1S 1 INZ(*ZEROS)
      /free
        line = 'V0 ' + %char(%size(V0)) + ' ' + %char(V0);
        write QSYSPRT line;
        line = 'V1 ' + %char(%size(V1)) + ' ' + %char(%len(V1)) + ' ' +
        %char(V1);
        write QSYSPRT line;
        line = 'V2 ' + %char(%size(V2)) + ' ' + %char(%len(V2)) + ' ' +
        %char(V2);
        write QSYSPRT line;
        line = 'V3 ' + %char(%size(V3)) + ' ' + %char(%len(V3)) + ' ' + '[' + V3
        + ']';
        write QSYSPRT line;
        line = 'V4 ' + %char(%size(V4)) + ' ' + %char(V4);
        write QSYSPRT line;
        line = 'V5 ' + %char(%size(V5(3))) + ' ' + %char(%len(V5(3))) + ' ' +
        %char(V5(3));
        write QSYSPRT line;
        line = 'V6 ' + %char(%size(V6(3))) + ' ' + %char(%len(V6(3))) + ' ' +
        %char(V6(3));
        write QSYSPRT line;
        line = 'V7 ' + %char(%size(V7)) + ' ' + %char(%len(V7)) + ' ' +
        %char(V7);
        write QSYSPRT line;
        *inlr = *on;
      /end-free
