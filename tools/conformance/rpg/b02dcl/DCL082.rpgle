     H DFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     D line            DS           132
     DDS1              DS                  ALIGN
     D S01                             T   TIMFMT(*USA)
     D S02                            4A   VARYING
     D S03                             D   DATFMT(*YMD.)
     D S04                            1S 0
     D S05                             D   DATFMT(*DMY/)
     D S06                            6S 2
     D S07                             Z
     D S08                             T   TIMFMT(*ISO)
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
        *inlr = *on;
      /end-free
