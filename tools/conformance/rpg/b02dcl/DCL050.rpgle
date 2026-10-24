     H DFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     D line            DS           132
     DDS1              DS                  INZ
     D S01                           11S 3
     D S02                            8F
     D S03                             *
     D S04                            1B 1
     D S05                            1A
     D S06                           10I 0
     D S07                            4F
     D S08                            2P 1
     D S09                           10U 0
      /free
        line = 'size ' + %char(%size(DS1));
        write QSYSPRT line;
        line = 'S01 ' + %char(%size(S01)) + ' ' + %char(%addr(S01) -
        %addr(DS1));
        write QSYSPRT line;
        line = '  ' + %char(S01);
        write QSYSPRT line;
        line = 'S02 ' + %char(%size(S02)) + ' ' + %char(%addr(S02) -
        %addr(DS1));
        write QSYSPRT line;
        line = '  ' + %char(S02);
        write QSYSPRT line;
        line = 'S03 ' + %char(%size(S03)) + ' ' + %char(%addr(S03) -
        %addr(DS1));
        write QSYSPRT line;
        line = '  ' + 'ptr';
        write QSYSPRT line;
        line = 'S04 ' + %char(%size(S04)) + ' ' + %char(%addr(S04) -
        %addr(DS1));
        write QSYSPRT line;
        line = '  ' + %char(S04);
        write QSYSPRT line;
        line = 'S05 ' + %char(%size(S05)) + ' ' + %char(%addr(S05) -
        %addr(DS1));
        write QSYSPRT line;
        line = '  ' + '[' + S05 + ']';
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
        line = '  ' + %char(S08);
        write QSYSPRT line;
        line = 'S09 ' + %char(%size(S09)) + ' ' + %char(%addr(S09) -
        %addr(DS1));
        write QSYSPRT line;
        line = '  ' + %char(S09);
        write QSYSPRT line;
        *inlr = *on;
      /end-free
