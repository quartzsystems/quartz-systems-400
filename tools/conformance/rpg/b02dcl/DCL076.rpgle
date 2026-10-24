     H DFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     D line            DS           132
     DDS1              DS
     D S01                             N
     D S02                           10I 0
     D S03                            4C
     D S04                            8F
     D S05                            8S 1
     D S06                             T   TIMFMT(*HMS:)
     D S07                             D   DATFMT(*YMD-)
     D S08                             T   TIMFMT(*USA)
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
        line = 'S07 ' + %char(%size(S07)) + ' ' + %char(%addr(S07) -
        %addr(DS1));
        write QSYSPRT line;
        line = 'S08 ' + %char(%size(S08)) + ' ' + %char(%addr(S08) -
        %addr(DS1));
        write QSYSPRT line;
        DS1 = 'AHOV29GNU18FMT07ELSZ6DKRY5CJQX4BIPW3AHOV29GNU18FMT07ELSZ6DKR';
        *inlr = *on;
      /end-free
