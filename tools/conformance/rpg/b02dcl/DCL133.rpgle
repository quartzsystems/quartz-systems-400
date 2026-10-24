     H DFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     D line            DS           132
     D PSDS           SDS
     D  PGMNAM                 1     10
     D  STS                   11     15  0
     D  PRVSTS                16     20  0
     D  RTN                   29     36
     D  NPARMS                37     39  0
     D  MSGID                 40     46
      /free
        line = %trim(PGMNAM) + ' ' + %char(STS) + ' ' + %char(PRVSTS) + ' ' +
        %trim(RTN) + ' ' +
        %char(NPARMS);
        write QSYSPRT line;
        line = '[' + MSGID + '] ' + %char(%size(PSDS));
        write QSYSPRT line;
        *inlr = *on;
      /end-free
