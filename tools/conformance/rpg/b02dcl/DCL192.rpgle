     H DFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     D line            DS           132
     DV0               S               D   DATFMT(*DMY.)
     DV1               S              2A   VARYING
     DV2               S              2P 2 INZ(-0.58)
     DV3               S              6B 1 INZ(36.2)
     DV4               S              1A
     DV5               S              2A   INZ(*ZEROS)
     DV6               S              5I 0 DIM(3) INZ(*LOVAL)
     DV7               S             10A   INZ(*BLANKS)
      /free
        line = 'V0 ' + %char(%size(V0)) + ' ' + %char(V0);
        write QSYSPRT line;
        line = 'V1 ' + %char(%size(V1)) + ' ' + %char(%len(V1)) + ' ' + '[' + V1
        + ']';
        write QSYSPRT line;
        line = 'V2 ' + %char(%size(V2)) + ' ' + %char(%len(V2)) + ' ' +
        %char(V2);
        write QSYSPRT line;
        line = 'V3 ' + %char(%size(V3)) + ' ' + %char(%len(V3)) + ' ' +
        %char(V3);
        write QSYSPRT line;
        line = 'V4 ' + %char(%size(V4)) + ' ' + %char(%len(V4)) + ' ' + '[' + V4
        + ']';
        write QSYSPRT line;
        line = 'V5 ' + %char(%size(V5)) + ' ' + %char(%len(V5)) + ' ' + '[' + V5
        + ']';
        write QSYSPRT line;
        line = 'V6 ' + %char(%size(V6(3))) + ' ' + %char(%len(V6(3))) + ' ' +
        %char(V6(3));
        write QSYSPRT line;
        line = 'V7 ' + %char(%size(V7)) + ' ' + %char(%len(V7)) + ' ' + '[' + V7
        + ']';
        write QSYSPRT line;
        *inlr = *on;
      /end-free
