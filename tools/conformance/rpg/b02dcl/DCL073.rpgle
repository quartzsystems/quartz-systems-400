     H DFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     D line            DS           132
     DDS1              DS
     D S01                             N
     D S02                            7A
     D S03                            9P 0 OVERLAY(S02)
     D S04                            5U 0
     D S05                            9B 2
     D S06                            7S 2 OVERLAY(S02)
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
        line = 'S05 ' + %char(%size(S05)) + ' ' + %char(%addr(S05) -
        %addr(DS1));
        write QSYSPRT line;
        line = 'S06 ' + %char(%size(S06)) + ' ' + %char(%addr(S06) -
        %addr(DS1));
        write QSYSPRT line;
        DS1 = 'AHOV29GNU18FMT07ELSZ6DKRY5CJQX4BIPW3AHOV29GNU18FMT07ELSZ6DKR';
        line = 'S02 [' + S02 + ']';
        write QSYSPRT line;
        *inlr = *on;
      /end-free
