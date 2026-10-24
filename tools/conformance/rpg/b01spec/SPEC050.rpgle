     hdftactgrp(*no)
     Fqsysprt   o    f  132        printer
     Dline             ds           132
     Dn                s              7p 2 inz(3.5)
     Dt                s              8a   inz('lower')
     C                   eval      line = %char(n) + ' ' + %trim(t)
     C                   write     qsysprt       line
     C                   eval      *inlr = *on
