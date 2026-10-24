     H DFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     D line            DS           132
     DV0               S              9B 1 INZ(*LOVAL)
     DV1               S               N
     DV2               S              3A   VARYING
     DV3               S              3S 1 INZ(*HIVAL)
     DV4               S              2A   VARYING
     DV5               S             10S 0 DIM(3) INZ(*LOVAL)
     DV6               S             13S 0 DIM(3)
     DV7               S              6C   INZ(U'0075')
      /free
        line = 'V0 ' + %char(%size(V0)) + ' ' + %char(%len(V0)) + ' ' +
        %char(V0);
        write QSYSPRT line;
        line = 'V1 ' + %char(%size(V1)) + ' ' + V1;
        write QSYSPRT line;
        line = 'V2 ' + %char(%size(V2)) + ' ' + %char(%len(V2)) + ' ' + '[' + V2
        + ']';
        write QSYSPRT line;
        line = 'V3 ' + %char(%size(V3)) + ' ' + %char(%len(V3)) + ' ' +
        %char(V3);
        write QSYSPRT line;
        line = 'V4 ' + %char(%size(V4)) + ' ' + %char(%len(V4)) + ' ' + '[' + V4
        + ']';
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
