     H DFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     D line            DS           132
     D MO              DS                  OCCURS(4) INZ
     D  A                             3
     D  B                             3  0
     D N               S              3  0
     C     2             OCCUR     MO
     C                   EVAL      A = 'two'
     C                   EVAL      B = 22
     C     4             OCCUR     MO
     C                   EVAL      A = 'for'
     C                   OCCUR     MO            N
     C     2             OCCUR     MO
      /free
        line = A + ' ' + %char(B) + ' ' + %char(N) + ' ' + %char(%size(MO :
        *ALL));
        write QSYSPRT line;
        %occur(MO) = 1;
        line = '[' + A + '] ' + %char(B);
        write QSYSPRT line;
        *inlr = *on;
      /end-free
