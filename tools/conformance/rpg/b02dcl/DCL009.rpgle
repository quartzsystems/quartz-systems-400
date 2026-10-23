     H DFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     D line            DS           132
     DF00              S               T   INZ(t'14.30.05') TIMFMT(*iso)
     DF01              S             34A   INZ('xyz') VARYING
     DF02              S              3I 0 INZ(30)
     DF03              S             20A   INZ('ab')
     DF04              S               D   DATFMT(*jul)
     DF05              S               T   TIMFMT(*eur)
     DF06              S             28S 2 INZ(295987008.46)
     DF07              S               Z
     DF08              S             12A   INZ('xyz') VARYING
     DF09              S              4F   INZ(100)
     DF10              S             15S 4 INZ(-292810849.0626)
     DF11              S              5B 3 INZ(10)
      /free
        line = 'F00 ' + %char(%size(F00)) + ' ' + %char(%len(F00));
        write QSYSPRT line;
        line = '  ' + '[' + %char(F00) + ']';
        write QSYSPRT line;
        line = 'F01 ' + %char(%size(F01)) + ' ' + %char(%len(F01));
        write QSYSPRT line;
        line = '  ' + '[' + %char(F01) + ']';
        write QSYSPRT line;
        line = 'F02 ' + %char(%size(F02)) + ' ' + %char(%len(F02)) + ' ' + %char(%decpos(F02));
        write QSYSPRT line;
        line = '  ' + %char(F02);
        write QSYSPRT line;
        line = 'F03 ' + %char(%size(F03)) + ' ' + %char(%len(F03));
        write QSYSPRT line;
        line = '  ' + '[' + %char(F03) + ']';
        write QSYSPRT line;
        line = 'F04 ' + %char(%size(F04)) + ' ' + %char(%len(F04));
        write QSYSPRT line;
        line = '  ' + '[' + %char(F04) + ']';
        write QSYSPRT line;
        line = 'F05 ' + %char(%size(F05)) + ' ' + %char(%len(F05));
        write QSYSPRT line;
        line = '  ' + '[' + %char(F05) + ']';
        write QSYSPRT line;
        line = 'F06 ' + %char(%size(F06)) + ' ' + %char(%len(F06)) + ' ' + %char(%decpos(F06));
        write QSYSPRT line;
        line = '  ' + %char(F06);
        write QSYSPRT line;
        line = 'F07 ' + %char(%size(F07)) + ' ' + %char(%len(F07));
        write QSYSPRT line;
        line = '  ' + '[' + %char(F07) + ']';
        write QSYSPRT line;
        line = 'F08 ' + %char(%size(F08)) + ' ' + %char(%len(F08));
        write QSYSPRT line;
        line = '  ' + '[' + %char(F08) + ']';
        write QSYSPRT line;
        line = 'F09 ' + %char(%size(F09)) + ' ' + %char(%len(F09)) + ' ' + %char(%decpos(F09));
        write QSYSPRT line;
        line = '  ' + %char(F09);
        write QSYSPRT line;
        line = 'F10 ' + %char(%size(F10)) + ' ' + %char(%len(F10)) + ' ' + %char(%decpos(F10));
        write QSYSPRT line;
        line = '  ' + %char(F10);
        write QSYSPRT line;
        line = 'F11 ' + %char(%size(F11)) + ' ' + %char(%len(F11)) + ' ' + %char(%decpos(F11));
        write QSYSPRT line;
        line = '  ' + %char(F11);
        write QSYSPRT line;
        *inlr = *on;
      /end-free
