     H DFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     D line            DS           132
     DV0               S              5I 0 INZ(56)
     DV1               S               D   DATFMT(*ISO) INZ(D'2026-03-05')
     DV2               S              6A   INZ(*BLANKS)
     DV3               S              5P 0 INZ(5154)
     DV4               S               T   TIMFMT(*HMS,) INZ(T'14.30.05')
     DV5               S              8A   VARYING DIM(3)
     DV6               S              4P 4 INZ(0.9164)
     DV7               S             14P 4 DIM(3) INZ(1552.5415)
      /free
        line = 'V0 ' + %char(%size(V0)) + ' ' + %char(%len(V0)) + ' ' +
        %char(V0);
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
        line = 'V5 ' + %char(%size(V5(3))) + ' ' + %char(%len(V5(3))) + ' ' +
        '[' + V5(3) + ']';
        write QSYSPRT line;
        line = 'V6 ' + %char(%size(V6)) + ' ' + %char(%len(V6)) + ' ' +
        %char(V6);
        write QSYSPRT line;
        line = 'V7 ' + %char(%size(V7(3))) + ' ' + %char(%len(V7(3))) + ' ' +
        %char(V7(3));
        write QSYSPRT line;
        *inlr = *on;
      /end-free
