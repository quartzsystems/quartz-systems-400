     H DFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     D line            DS           132
     DV0               S             12A   DIM(3) INZ('KQZWMX')
     DV1               S              3U 0 INZ(75)
     DV2               S              7B 0
     DV3               S              6A   INZ('KQZWMX')
     DV4               S              7S 3 INZ(2049.334)
     DV5               S              3U 0 INZ(*ZEROS)
     DV6               S               Z   INZ(Z'2026-03-05-14.30.05.000123')
     DV7               S               N   INZ(*ON)
      /free
        line = 'V0 ' + %char(%size(V0(3))) + ' ' + %char(%len(V0(3))) + ' ' +
        '[' + V0(3) + ']';
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
        line = 'V4 ' + %char(%size(V4)) + ' ' + %char(%len(V4)) + ' ' +
        %char(V4);
        write QSYSPRT line;
        line = 'V5 ' + %char(%size(V5)) + ' ' + %char(%len(V5)) + ' ' +
        %char(V5);
        write QSYSPRT line;
        line = 'V6 ' + %char(%size(V6)) + ' ' + %char(V6);
        write QSYSPRT line;
        line = 'V7 ' + %char(%size(V7)) + ' ' + V7;
        write QSYSPRT line;
        *inlr = *on;
      /end-free
