     H DFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     D line            DS           132
     DV0               S             15P 4 INZ(6352.4184)
     DV1               S             13P 0 DIM(3) INZ(*LOVAL)
     DV2               S               T   TIMFMT(*USA) DIM(3) INZ(T'14.30.05')
     DV3               S              5A
     DV4               S              8A   VARYING INZ('VW')
     DV5               S             15S 3 INZ(*HIVAL)
     DV6               S               Z   INZ(*LOVAL)
     DV7               S             11A   INZ('K')
      /free
        line = 'V0 ' + %char(%size(V0)) + ' ' + %char(%len(V0)) + ' ' +
        %char(V0);
        write QSYSPRT line;
        line = 'V1 ' + %char(%size(V1(3))) + ' ' + %char(%len(V1(3))) + ' ' +
        %char(V1(3));
        write QSYSPRT line;
        line = 'V2 ' + %char(%size(V2(3))) + ' ' + %char(V2(3));
        write QSYSPRT line;
        line = 'V3 ' + %char(%size(V3)) + ' ' + %char(%len(V3)) + ' ' + '[' + V3
        + ']';
        write QSYSPRT line;
        line = 'V4 ' + %char(%size(V4)) + ' ' + %char(%len(V4)) + ' ' + '[' + V4
        + ']';
        write QSYSPRT line;
        line = 'V5 ' + %char(%size(V5)) + ' ' + %char(%len(V5)) + ' ' +
        %char(V5);
        write QSYSPRT line;
        line = 'V6 ' + %char(%size(V6)) + ' ' + %char(V6);
        write QSYSPRT line;
        line = 'V7 ' + %char(%size(V7)) + ' ' + %char(%len(V7)) + ' ' + '[' + V7
        + ']';
        write QSYSPRT line;
        *inlr = *on;
      /end-free
