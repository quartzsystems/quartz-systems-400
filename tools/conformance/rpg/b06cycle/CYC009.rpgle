      *; dds: SALESF
      *; sqlfile: SALESF
      * Fetch overflow on detail lines, totals at L1 and LR.
     FSALESF    IP   E             DISK
     FQSYSPRT   O    F  132        PRINTER OFLIND(*INOF)
     DREGTOT           S              9P 2
     ISALESR        01
     I              REGION                      REGION        L1
     C   01              ADD       AMT           REGTOT
     OQSYSPRT   H    1P                     2
     O         OR    OF
     O                                           13 'Region report'
     O                       PAGE          Z     30
     OQSYSPRT   DF   01                     4
     O                       REGION               4
     O                       REP                 10
     O                       ITEM                22
     O                       AMT           J     34
     OQSYSPRT   DF   01                     2
     O                       QTY           Z     10
     OQSYSPRT   T    L1                     2
     O                                            6 'Region'
     O                       REGION               9
     O                       REGTOT        JB    34
     OQSYSPRT   T    LR
     O                                            5 'Pages'
     O                       PAGE          Z     12
