     H DFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     D line            DS           132
     DF00              S             25S 7 INZ(475344818.2077169)
     DF01              S              4F
     DF02              S             18B 2 INZ(5965)
     DF03              S              3I 0
     DF04              S               T   INZ(t'14.30.05') TIMFMT(*jis)
     DF05              S              1N
     DF06              S             20S 6 INZ(267356335.324827)
     DF07              S              4F   INZ(-2.25)
     DF08              S              3A   VARYING
     DF09              S             39A
     DF10              S             17B 2 INZ(6424)
     DF11              S              6A
      /free
        line = 'F00 ' + %char(%size(F00)) + ' ' + %char(%len(F00)) + ' ' + %char(%decpos(F00));
        write QSYSPRT line;
        line = '  ' + %char(F00);
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
        line = 'F07 ' + %char(%size(F07)) + ' ' + %char(%len(F07)) + ' ' + %char(%decpos(F07));
        write QSYSPRT line;
        line = '  ' + %char(F07);
        write QSYSPRT line;
        line = 'F08 ' + %char(%size(F08)) + ' ' + %char(%len(F08));
        write QSYSPRT line;
        line = '  ' + '[' + %char(F08) + ']';
        write QSYSPRT line;
        line = 'F09 ' + %char(%size(F09)) + ' ' + %char(%len(F09));
        write QSYSPRT line;
        line = '  ' + '[' + %char(F09) + ']';
        write QSYSPRT line;
        line = 'F10 ' + %char(%size(F10)) + ' ' + %char(%len(F10)) + ' ' + %char(%decpos(F10));
        write QSYSPRT line;
        line = '  ' + %char(F10);
        write QSYSPRT line;
        line = 'F11 ' + %char(%size(F11)) + ' ' + %char(%len(F11));
        write QSYSPRT line;
        line = '  ' + '[' + %char(F11) + ']';
        write QSYSPRT line;
        *inlr = *on;
      /end-free
