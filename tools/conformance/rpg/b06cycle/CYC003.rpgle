      *; dds: SALESF
      *; sqlfile: SALESF
      * One control level: region totals at L1, a grand total at LR.
     FSALESF    IP   F   22        DISK
     FQSYSPRT   O    F  132        PRINTER OFLIND(*INOF)
     DREGTOT           S              9P 2
     DGRAND            S             11P 2
     ISALESF    NS  01
     I                                  1    2  REGION        L1
     I                                  3    6  REP
     I                                  7   16  ITEM
     I                             P   17   20 2AMT
     I                             P   21   22 0QTY
     C   01              ADD       AMT           REGTOT
     CL1                 ADD       REGTOT        GRAND
     OQSYSPRT   D    01                     1
     O                       REGION               4
     O                       ITEM                16
     O                       AMT           J     28
     OQSYSPRT   D    L1                  1  1
     O                                           12 'New region'
     O                       REGION              15
     OQSYSPRT   T    L1                     1
     O                                           10 'Total for'
     O                       REGION              13
     O                       REGTOT        JB    28
     OQSYSPRT   T    LR                  1
     O                                           11 'Grand total'
     O                       GRAND         J     28
