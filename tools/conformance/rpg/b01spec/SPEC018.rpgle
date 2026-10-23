     fqsysprt   o    f 132         printer
     dline             ds           132
     dn                s            5    0 inz(31)
     c                   eval      line = 'lower ' + %char(n)
     c                   write     qsysprt       line
     c                   eval      *inlr = *on
