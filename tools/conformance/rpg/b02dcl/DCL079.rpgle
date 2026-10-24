     H DFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     D line            DS           132
     DDS1              DS                  ALIGN
     D S01                            7A   VARYING
     D S02                            7A
     D S03                             *
     D S04                            4F
     D S05                            8F
     D S06                           10I 0
     D S07                             D   DATFMT(*EUR)
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
        *inlr = *on;
      /end-free
