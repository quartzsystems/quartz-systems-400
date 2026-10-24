     H DFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     D line            DS           132
     DV0               S             14S 3 INZ(*ZEROS)
     DV1               S              2C
     DV2               S              9A   INZ('KQ')
     DV3               S              5B 2 DIM(3) INZ(*ZEROS)
     DV4               S               Z   INZ(Z'2026-03-05-14.30.05.000123')
     DV5               S              9A
     DV6               S              3A   VARYING INZ('VW')
     DV7               S               D   DATFMT(*MDY/) INZ(D'2026-03-05')
      /free
        line = 'V0 ' + %char(%size(V0)) + ' ' + %char(%len(V0)) + ' ' +
        %char(V0);
        write QSYSPRT line;
        line = 'V1 ' + %char(%size(V1)) + ' ' + %char(%len(V1)) + ' ' +
        %char(V1);
        write QSYSPRT line;
        line = 'V2 ' + %char(%size(V2)) + ' ' + %char(%len(V2)) + ' ' + '[' + V2
        + ']';
        write QSYSPRT line;
        line = 'V3 ' + %char(%size(V3(3))) + ' ' + %char(%len(V3(3))) + ' ' +
        %char(V3(3));
        write QSYSPRT line;
        line = 'V4 ' + %char(%size(V4)) + ' ' + %char(V4);
        write QSYSPRT line;
        line = 'V5 ' + %char(%size(V5)) + ' ' + %char(%len(V5)) + ' ' + '[' + V5
        + ']';
        write QSYSPRT line;
        line = 'V6 ' + %char(%size(V6)) + ' ' + %char(%len(V6)) + ' ' + '[' + V6
        + ']';
        write QSYSPRT line;
        line = 'V7 ' + %char(%size(V7)) + ' ' + %char(V7);
        write QSYSPRT line;
        *inlr = *on;
      /end-free
