     Fqsysprt   o    f   80        printer                                      80-byte lines
     Dline             ds            80                                         the line
     C                   eval      line = 'eighty ' + %char(%size(line))        set it
     C                   write     qsysprt       line                           print it
     C                   eval      *inlr = *on
