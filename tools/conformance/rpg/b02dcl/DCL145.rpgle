     H DFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     D line            DS           132
     D BASE            S              9P 3 INZ(1.234)
     D UP              S             +2    LIKE(BASE)
     D DOWN            S             -4    LIKE(BASE)
     D CH              S             10    INZ('ten')
     D CH2             S             -5    LIKE(CH) INZ('five')
     D DS1             DS
     D  SUB                    1      6P 0
     D SUB2                          +1    LIKE(SUB)
      /free
        line = %char(%len(UP)) + ' ' + %char(%decpos(UP)) + ' ' +
        %char(%len(DOWN)) + ' ' +
        %char(%len(CH2)) + ' [' + CH2 + '] ' + %char(%len(SUB2)) + ' ' +
        %char(%size(DS1));
        write QSYSPRT line;
        *inlr = *on;
      /end-free
