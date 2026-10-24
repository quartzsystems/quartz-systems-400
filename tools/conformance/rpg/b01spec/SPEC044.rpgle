     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     Drec              ds                  qualified
     D   code                         3    inz('abc')
     D     qty                        5p 0 inz(12)
     D  flag                           n   inz(*on)
     C                   EVAL      LINE = rec.code + ' ' + %char(rec.qty) + ' '
     C                             + rec.flag + ' ' + %char(%size(rec))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
