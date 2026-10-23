     FQSYSPRT   O    F  132        PRINTER OFLIND(*INOF)
     DAMOUNT           S              7  2 INZ(1234.5)
     DNAME             S             10    INZ('Widget')
     C                   EXCEPT    HDR
     C                   EXCEPT    DTL
     C                   EVAL      AMOUNT = -42.75
     C                   EXCEPT    DTL
     C                   EVAL      *INLR = *ON
     OQSYSPRT   E            HDR            2
     O                                           10 'Item'
     O                                           30 'Amount'
     O          E            DTL            1
     O                       NAME                12
     O                       AMOUNT        J     30
     O                       AMOUNT        1     45
