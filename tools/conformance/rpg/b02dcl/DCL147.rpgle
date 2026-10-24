     H DFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     D line            DS           132
     D LONG            C                   'The beginning of a long constant +
     D                                     which goes on for a while, then +
     D                                     ends here.'
     D DASH            C                   'dash-
     D                                       continued'
     D NUM             C                   CONST(-123.456)
      /free
        line = %subst(LONG : 1 : 50);
        write QSYSPRT line;
        line = %subst(LONG : 51) + ' ' + %char(%len(LONG));
        write QSYSPRT line;
        line = '[' + DASH + '] ' + %char(NUM);
        write QSYSPRT line;
        *inlr = *on;
      /end-free
