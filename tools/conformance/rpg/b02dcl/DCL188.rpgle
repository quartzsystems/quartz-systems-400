     H DFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     D line            DS           132
     DV0               S               N   INZ(*OFF)
     DV1               S              5U 0 INZ(*ZEROS)
     DV2               S              9A   INZ('KQZWM')
     DV3               S               D   DATFMT(*JUL&)
     DV4               S             10I 0 INZ(67)
     DV5               S             11P 3 INZ(431.852)
     DV6               S              6B 2 INZ(*ZEROS)
     DV7               S             11P 1 INZ(952.5)
      /free
        line = 'V0 ' + %char(%size(V0)) + ' ' + V0;
        write QSYSPRT line;
        line = 'V1 ' + %char(%size(V1)) + ' ' + %char(%len(V1)) + ' ' +
        %char(V1);
        write QSYSPRT line;
        line = 'V2 ' + %char(%size(V2)) + ' ' + %char(%len(V2)) + ' ' + '[' + V2
        + ']';
        write QSYSPRT line;
        line = 'V3 ' + %char(%size(V3)) + ' ' + %char(V3);
        write QSYSPRT line;
        line = 'V4 ' + %char(%size(V4)) + ' ' + %char(%len(V4)) + ' ' +
        %char(V4);
        write QSYSPRT line;
        line = 'V5 ' + %char(%size(V5)) + ' ' + %char(%len(V5)) + ' ' +
        %char(V5);
        write QSYSPRT line;
        line = 'V6 ' + %char(%size(V6)) + ' ' + %char(%len(V6)) + ' ' +
        %char(V6);
        write QSYSPRT line;
        line = 'V7 ' + %char(%size(V7)) + ' ' + %char(%len(V7)) + ' ' +
        %char(V7);
        write QSYSPRT line;
        *inlr = *on;
      /end-free
