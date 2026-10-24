     H DFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     D line            DS           132
     D REC             DS            30    QUALIFIED
     D  A                      1      4
     D  B                      5      8P 2
     D  C                      9     12I 0
     D  D                     13     14U 0
     D  E                     15     18F
     D  F                     19     20B 0
     D  G                     21     23S 1
     D  H                     24     24N
     D  I                     25     30
      /free
        REC.B = -12.34;
        REC.C = 99;
        REC.D = 65535;
        REC.E = 2.5;
        REC.F = -3;
        REC.G = 4.5;
        REC.H = *on;
        line = %char(REC.B) + ' ' + %char(REC.C) + ' ' + %char(REC.D) + ' ' +
        %char(REC.E) + ' ' +
        %char(REC.F) + ' ' + %char(REC.G) + ' ' + REC.H;
        write QSYSPRT line;
        line = %char(%len(REC.B)) + ' ' + %char(%len(REC.C)) + ' ' +
        %char(%len(REC.D)) + ' ' +
        %char(%size(REC.E)) + ' ' + %char(%len(REC.F)) + ' ' +
        %char(%len(REC.G)) + ' ' +
        %char(%size(REC));
        write QSYSPRT line;
        *inlr = *on;
      /end-free
