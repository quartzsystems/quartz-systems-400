     H DFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     D line            DS           132
     DF00              S              9B 2 INZ(5965)
     DF01              S              3I 0
     DF02              S               T   INZ(t'14.30.05') TIMFMT(*jis)
     DF03              S              1N
     DF04              S             20S 6 INZ(267356335.324827)
     DF05              S              4F   INZ(-2.25)
     DF06              S              3A   VARYING
     DF07              S             39A
     DF08              S              9B 2 INZ(6424)
     DF09              S              6A
     DF10              S             30S 5
     DF11              S               D   INZ(d'2026-03-05') DATFMT(*jul)
      /free
        line = 'F00 ' + %char(%size(F00)) + ' ' + %char(%len(F00)) + ' ' +
        %char(%decpos(F00));
        write QSYSPRT line;
        line = '  ' + %char(F00);
        write QSYSPRT line;
        line = 'F01 ' + %char(%size(F01)) + ' ' + %char(%len(F01)) + ' ' +
        %char(%decpos(F01));
        write QSYSPRT line;
        line = '  ' + %char(F01);
        write QSYSPRT line;
        line = 'F02 ' + %char(%size(F02)) + ' ' + %char(%len(F02));
        write QSYSPRT line;
        line = '  ' + '[' + %char(F02) + ']';
        write QSYSPRT line;
        line = 'F03 ' + %char(%size(F03)) + ' ' + %char(%len(F03));
        write QSYSPRT line;
        line = '  ' + '[' + %char(F03) + ']';
        write QSYSPRT line;
        line = 'F04 ' + %char(%size(F04)) + ' ' + %char(%len(F04)) + ' ' +
        %char(%decpos(F04));
        write QSYSPRT line;
        line = '  ' + %char(F04);
        write QSYSPRT line;
        line = 'F05 ' + %char(%size(F05)) + ' ' + %char(%len(F05));
        write QSYSPRT line;
        line = '  ' + %char(F05);
        write QSYSPRT line;
        line = 'F06 ' + %char(%size(F06)) + ' ' + %char(%len(F06));
        write QSYSPRT line;
        line = '  ' + '[' + %char(F06) + ']';
        write QSYSPRT line;
        line = 'F07 ' + %char(%size(F07)) + ' ' + %char(%len(F07));
        write QSYSPRT line;
        line = '  ' + '[' + %char(F07) + ']';
        write QSYSPRT line;
        line = 'F08 ' + %char(%size(F08)) + ' ' + %char(%len(F08)) + ' ' +
        %char(%decpos(F08));
        write QSYSPRT line;
        line = '  ' + %char(F08);
        write QSYSPRT line;
        line = 'F09 ' + %char(%size(F09)) + ' ' + %char(%len(F09));
        write QSYSPRT line;
        line = '  ' + '[' + %char(F09) + ']';
        write QSYSPRT line;
        line = 'F10 ' + %char(%size(F10)) + ' ' + %char(%len(F10)) + ' ' +
        %char(%decpos(F10));
        write QSYSPRT line;
        line = '  ' + %char(F10);
        write QSYSPRT line;
        line = 'F11 ' + %char(%size(F11)) + ' ' + %char(%len(F11));
        write QSYSPRT line;
        line = '  ' + '[' + %char(F11) + ']';
        write QSYSPRT line;
        *inlr = *on;
      /end-free
