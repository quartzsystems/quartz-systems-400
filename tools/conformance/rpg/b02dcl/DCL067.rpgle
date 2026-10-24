     H DFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     D line            DS           132
     DDS1              DS
     D S01                             T   TIMFMT(*ISO)
     D S02                             D   DATFMT(*JUL/)
     D S03                           20I 0
     D S04                            1P 0
      /free
        line = 'size ' + %char(%size(DS1));
        write QSYSPRT line;
        line = 'S01 ' + %char(%size(S01)) + ' ' + %char(%addr(S01) -
        %addr(DS1));
        write QSYSPRT line;
        line = 'S02 ' + %char(%size(S02)) + ' ' + %char(%addr(S02) -
        %addr(DS1));
        write QSYSPRT line;
        line = 'S03 ' + %char(%size(S03)) + ' ' + %char(%addr(S03) -
        %addr(DS1));
        write QSYSPRT line;
        line = 'S04 ' + %char(%size(S04)) + ' ' + %char(%addr(S04) -
        %addr(DS1));
        write QSYSPRT line;
        DS1 = 'AHOV29GNU18FMT07ELSZ6DKRY5CJQX4BIPW3AHOV29GNU18FMT07ELSZ6DKR';
        *inlr = *on;
      /end-free
