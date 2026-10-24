     H DFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     D line            DS           132
     DV0               S              7A   INZ(*ZEROS)
     DV1               S               T   TIMFMT(*EUR)
     DV2               S             12A
     DV3               S              1C   INZ(U'0075')
     DV4               S               D   DATFMT(*JUL.) INZ(D'2026-03-05')
     DV5               S             10A   INZ(*ZEROS)
     DV6               S              1B 1 INZ(0.7)
     DV7               S              1S 1 INZ(-0.9)
      /free
        line = 'V0 ' + %char(%size(V0)) + ' ' + %char(%len(V0)) + ' ' + '[' + V0
        + ']';
        write QSYSPRT line;
        line = 'V1 ' + %char(%size(V1)) + ' ' + %char(V1);
        write QSYSPRT line;
        line = 'V2 ' + %char(%size(V2)) + ' ' + %char(%len(V2)) + ' ' + '[' + V2
        + ']';
        write QSYSPRT line;
        line = 'V3 ' + %char(%size(V3)) + ' ' + %char(%len(V3)) + ' ' +
        %char(V3);
        write QSYSPRT line;
        line = 'V4 ' + %char(%size(V4)) + ' ' + %char(V4);
        write QSYSPRT line;
        line = 'V5 ' + %char(%size(V5)) + ' ' + %char(%len(V5)) + ' ' + '[' + V5
        + ']';
        write QSYSPRT line;
        line = 'V6 ' + %char(%size(V6)) + ' ' + %char(%len(V6)) + ' ' +
        %char(V6);
        write QSYSPRT line;
        line = 'V7 ' + %char(%size(V7)) + ' ' + %char(%len(V7)) + ' ' +
        %char(V7);
        write QSYSPRT line;
        *inlr = *on;
      /end-free
