     H DFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     D line            DS           132
     DDS1              DS                  INZ
     D S01                            5A
     D S02                             Z
     D S03                             T   TIMFMT(*EUR)
     D S04                           20I 0
     D S05                            4C
     D S06                             Z
     D S07                            1P 0
     D S08                             *
     D S09                            2C
      /free
        line = 'size ' + %char(%size(DS1));
        write QSYSPRT line;
        line = 'S01 ' + %char(%size(S01)) + ' ' + %char(%addr(S01) -
        %addr(DS1));
        write QSYSPRT line;
        line = '  ' + '[' + S01 + ']';
        write QSYSPRT line;
        line = 'S02 ' + %char(%size(S02)) + ' ' + %char(%addr(S02) -
        %addr(DS1));
        write QSYSPRT line;
        line = '  ' + %char(S02);
        write QSYSPRT line;
        line = 'S03 ' + %char(%size(S03)) + ' ' + %char(%addr(S03) -
        %addr(DS1));
        write QSYSPRT line;
        line = '  ' + %char(S03);
        write QSYSPRT line;
        line = 'S04 ' + %char(%size(S04)) + ' ' + %char(%addr(S04) -
        %addr(DS1));
        write QSYSPRT line;
        line = '  ' + %char(S04);
        write QSYSPRT line;
        line = 'S05 ' + %char(%size(S05)) + ' ' + %char(%addr(S05) -
        %addr(DS1));
        write QSYSPRT line;
        line = '  ' + %char(S05);
        write QSYSPRT line;
        line = 'S06 ' + %char(%size(S06)) + ' ' + %char(%addr(S06) -
        %addr(DS1));
        write QSYSPRT line;
        line = '  ' + %char(S06);
        write QSYSPRT line;
        line = 'S07 ' + %char(%size(S07)) + ' ' + %char(%addr(S07) -
        %addr(DS1));
        write QSYSPRT line;
        line = '  ' + %char(S07);
        write QSYSPRT line;
        line = 'S08 ' + %char(%size(S08)) + ' ' + %char(%addr(S08) -
        %addr(DS1));
        write QSYSPRT line;
        line = '  ' + 'ptr';
        write QSYSPRT line;
        line = 'S09 ' + %char(%size(S09)) + ' ' + %char(%addr(S09) -
        %addr(DS1));
        write QSYSPRT line;
        line = '  ' + %char(S09);
        write QSYSPRT line;
        *inlr = *on;
      /end-free
