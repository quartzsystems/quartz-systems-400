      * Fixed and free in one member.
      *; run: no
     HDFTACTGRP(*NO) DATFMT(*ISO)
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DN                S              7P 2 INZ(1.5)
     DNAME             S             10    INZ('mix')
       dcl-s extra int(10) inz(4);
      /free
        n = n * 2;
        if n > 2;
          name = 'big';
        endif;
      /end-free
       n += extra;
     C     N             ADD       1             N
     C                   EVAL      LINE = %TRIM(NAME) + ' ' + %CHAR(N)
     C                   WRITE     QSYSPRT       LINE
     C                   SETON                                        LR
