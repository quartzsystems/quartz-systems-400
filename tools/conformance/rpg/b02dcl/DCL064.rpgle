     H DFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     D line            DS           132
     DDS1              DS
     D S01                           20U 0
     D S02                            4A   VARYING
     D S03                             Z
     D S04                           15P 1
     D S05                             Z
     D S06                             N
     D S07                            5S 1
     D S08                            8F
     D S09                             N
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
        line = 'S09 ' + %char(%size(S09)) + ' ' + %char(%addr(S09) -
        %addr(DS1));
        write QSYSPRT line;
        DS1 = 'AHOV29GNU18FMT07ELSZ6DKRY5CJQX4BIPW3AHOV29GNU18FMT07ELSZ6DKR';
        *inlr = *on;
      /end-free
