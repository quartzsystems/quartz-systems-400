     H DFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     D line            DS           132
     DF00              S               Z   INZ(z'2026-03-05-14.30.05.123456')
     DF01              S              3U 0
     DF02              S             21P 5 INZ(218259096.98223)
     DF03              S              8B 4 INZ(5336)
     DF04              S              4A   INZ('')
     DF05              S               D   INZ(d'2026-03-05') DATFMT(*usa)
     DF06              S               D   DATFMT(*jis)
     DF07              S             26A   INZ('abcd')
     DF08              S              6B 2 INZ(7180)
     DF09              S              3U 0 INZ(29)
     DF10              S             26P 0 INZ(-829880011)
     DF11              S              3S 2 INZ(6.31)
      /free
        line = 'F00 ' + %char(%size(F00)) + ' ' + %char(%len(F00));
        write QSYSPRT line;
        line = '  ' + '[' + %char(F00) + ']';
        write QSYSPRT line;
        line = 'F01 ' + %char(%size(F01)) + ' ' + %char(%len(F01)) + ' ' +
        %char(%decpos(F01));
        write QSYSPRT line;
        line = '  ' + %char(F01);
        write QSYSPRT line;
        line = 'F02 ' + %char(%size(F02)) + ' ' + %char(%len(F02)) + ' ' +
        %char(%decpos(F02));
        write QSYSPRT line;
        line = '  ' + %char(F02);
        write QSYSPRT line;
        line = 'F03 ' + %char(%size(F03)) + ' ' + %char(%len(F03)) + ' ' +
        %char(%decpos(F03));
        write QSYSPRT line;
        line = '  ' + %char(F03);
        write QSYSPRT line;
        line = 'F04 ' + %char(%size(F04)) + ' ' + %char(%len(F04));
        write QSYSPRT line;
        line = '  ' + '[' + %char(F04) + ']';
        write QSYSPRT line;
        line = 'F05 ' + %char(%size(F05)) + ' ' + %char(%len(F05));
        write QSYSPRT line;
        line = '  ' + '[' + %char(F05) + ']';
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
        line = 'F09 ' + %char(%size(F09)) + ' ' + %char(%len(F09)) + ' ' +
        %char(%decpos(F09));
        write QSYSPRT line;
        line = '  ' + %char(F09);
        write QSYSPRT line;
        line = 'F10 ' + %char(%size(F10)) + ' ' + %char(%len(F10)) + ' ' +
        %char(%decpos(F10));
        write QSYSPRT line;
        line = '  ' + %char(F10);
        write QSYSPRT line;
        line = 'F11 ' + %char(%size(F11)) + ' ' + %char(%len(F11)) + ' ' +
        %char(%decpos(F11));
        write QSYSPRT line;
        line = '  ' + %char(F11);
        write QSYSPRT line;
        *inlr = *on;
      /end-free
