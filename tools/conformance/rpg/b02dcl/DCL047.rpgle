     H DFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     D line            DS           132
     DDS1              DS                  INZ
     D S01                            5C
     D S02                            3S 1
     D S03                            8F
     D S04                            6A   VARYING
     D S05                            5A   VARYING
     D S06                            3A
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
        line = '  ' + %char(S03);
        write QSYSPRT line;
        line = 'S04 ' + %char(%size(S04)) + ' ' + %char(%addr(S04) -
        %addr(DS1));
        write QSYSPRT line;
        line = '  ' + '[' + S04 + ']';
        write QSYSPRT line;
        line = 'S05 ' + %char(%size(S05)) + ' ' + %char(%addr(S05) -
        %addr(DS1));
        write QSYSPRT line;
        line = '  ' + '[' + S05 + ']';
        write QSYSPRT line;
        line = 'S06 ' + %char(%size(S06)) + ' ' + %char(%addr(S06) -
        %addr(DS1));
        write QSYSPRT line;
        line = '  ' + '[' + S06 + ']';
        write QSYSPRT line;
        *inlr = *on;
      /end-free
