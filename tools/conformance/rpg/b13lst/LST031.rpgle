      * Output buffer positions.
      *; run: no
     FQSYSPRT   O    F  132        PRINTER OFLIND(*INOF)
     DAMT              S              9P 2 INZ(-1234.5)
     DCNT              S              5P 0 INZ(42)
     DNAME             S              8    INZ('report')
     C                   EXCEPT    DTL
     C                   SETON                                        LR
     OQSYSPRT   H    1P                     2  1
     O                                           10 'Title'
     O                       PAGE          Z     +3
     OQSYSPRT   E            DTL            1
     O                       NAME                10
     O                       AMT           J     +2
     O                       CNT           Z     40
     O                       AMT                 60 '  ,   , 0 .  -'
     O                       UDATE         Y     75
