     H DFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     D line            DS           132
     DF00              S              7A   INZ('xyz') VARYING
     DF01              S              3U 0 INZ(28)
     DF02              S              3I 0 INZ(76)
     DF03              S             19S 2 INZ(852755170.86)
     DF04              S              3B 1 INZ(77)
     DF05              S             25P 8
     DF06              S             10U 0
     DF07              S             18P 4
     DF08              S               D   INZ(d'2026-03-05') DATFMT(*usa)
     DF09              S              2B 2
     DF10              S             20A   INZ('xyz') VARYING
     DF11              S              4F
      /free
        line = 'F00 ' + %char(%size(F00)) + ' ' + %char(%len(F00));
        write QSYSPRT line;
        line = '  ' + '[' + %char(F00) + ']';
        write QSYSPRT line;
        line = 'F01 ' + %char(%size(F01)) + ' ' + %char(%len(F01)) + ' ' + %char(%decpos(F01));
        write QSYSPRT line;
        line = '  ' + %char(F01);
        write QSYSPRT line;
        line = 'F02 ' + %char(%size(F02)) + ' ' + %char(%len(F02)) + ' ' + %char(%decpos(F02));
        write QSYSPRT line;
        line = '  ' + %char(F02);
        write QSYSPRT line;
        line = 'F03 ' + %char(%size(F03)) + ' ' + %char(%len(F03)) + ' ' + %char(%decpos(F03));
        write QSYSPRT line;
        line = '  ' + %char(F03);
        write QSYSPRT line;
        line = 'F04 ' + %char(%size(F04)) + ' ' + %char(%len(F04)) + ' ' + %char(%decpos(F04));
        write QSYSPRT line;
        line = '  ' + %char(F04);
        write QSYSPRT line;
        line = 'F05 ' + %char(%size(F05)) + ' ' + %char(%len(F05)) + ' ' + %char(%decpos(F05));
        write QSYSPRT line;
        line = '  ' + %char(F05);
        write QSYSPRT line;
        line = 'F06 ' + %char(%size(F06)) + ' ' + %char(%len(F06)) + ' ' + %char(%decpos(F06));
        write QSYSPRT line;
        line = '  ' + %char(F06);
        write QSYSPRT line;
        line = 'F07 ' + %char(%size(F07)) + ' ' + %char(%len(F07)) + ' ' + %char(%decpos(F07));
        write QSYSPRT line;
        line = '  ' + %char(F07);
        write QSYSPRT line;
        line = 'F08 ' + %char(%size(F08)) + ' ' + %char(%len(F08));
        write QSYSPRT line;
        line = '  ' + '[' + %char(F08) + ']';
        write QSYSPRT line;
        line = 'F09 ' + %char(%size(F09)) + ' ' + %char(%len(F09)) + ' ' + %char(%decpos(F09));
        write QSYSPRT line;
        line = '  ' + %char(F09);
        write QSYSPRT line;
        line = 'F10 ' + %char(%size(F10)) + ' ' + %char(%len(F10));
        write QSYSPRT line;
        line = '  ' + '[' + %char(F10) + ']';
        write QSYSPRT line;
        line = 'F11 ' + %char(%size(F11)) + ' ' + %char(%len(F11)) + ' ' + %char(%decpos(F11));
        write QSYSPRT line;
        line = '  ' + %char(F11);
        write QSYSPRT line;
        *inlr = *on;
      /end-free
