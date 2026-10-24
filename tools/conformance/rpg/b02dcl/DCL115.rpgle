     H DFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     D line            DS           132
     D G1              S              5G
     D U1              S              5C
     D U2              S              5C   VARYING(4)
     D V1              S          65535A   VARYING
     D V2              S             10A   VARYING(2)
      /free
        line = %char(%size(G1)) + ' ' + %char(%size(U1)) + ' ' +
        %char(%size(U2)) + ' ' +
        %char(%size(V1)) + ' ' + %char(%size(V2));
        write QSYSPRT line;
        *inlr = *on;
      /end-free
