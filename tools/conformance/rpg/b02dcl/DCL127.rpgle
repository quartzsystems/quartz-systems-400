     H DFTACTGRP(*NO)
      *; dds: TYPESF
     FQSYSPRT   O    F  132        PRINTER
     D line            DS           132
     D TY            E DS                  EXTNAME(TYPESF)
     D TK            E DS                  EXTNAME(TYPESF:*KEY)
     D                                     QUALIFIED
     D TQ            E DS                  EXTNAME(TYPESF) QUALIFIED
     D                                     PREFIX(Q_)
      /free
        line = %char(%size(TY)) + ' ' + %char(%size(TK)) + ' ' +
        %char(%size(TQ));
        write QSYSPRT line;
        line = %char(%size(TDATE)) + ' ' + %char(%size(TTIME)) + ' ' +
        %char(%size(TSTMP)) + ' ' +
        %char(%size(TVAR));
        write QSYSPRT line;
        line = %char(%size(TBIN)) + ' ' + %char(%size(TBIN2)) + ' ' +
        %char(%size(TQ.Q_TZON));
        write QSYSPRT line;
        *inlr = *on;
      /end-free
