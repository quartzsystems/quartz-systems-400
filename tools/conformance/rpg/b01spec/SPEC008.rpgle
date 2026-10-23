     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DCOUNT            S             10I 0
       dcl-s name char(10) inz('mixed');
     C                   EVAL      COUNT = 2
       count += 3;
       line = 'count=' + %char(count) + ' name=' + %trim(name);
     C                   WRITE     QSYSPRT       LINE
       if count = 5;
         line = 'five';
         write QSYSPRT line;
       endif;
       *inlr = *on;
