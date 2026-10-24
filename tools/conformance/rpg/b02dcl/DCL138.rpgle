     H DFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     D line            DS           132
     D LDA            UDS                  DTAARA(*LDA)
     D  LFLD                   1     10
     D  LNUM                  11     15  0
      /free
        LFLD = 'lda field';
        LNUM = 12345;
        line = LFLD + ' ' + %char(LNUM) + ' ' + %char(%size(LDA));
        write QSYSPRT line;
        *inlr = *on;
      /end-free
