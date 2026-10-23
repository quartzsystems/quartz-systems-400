      *; dds: SALESF
      *; sqlfile: SALESF
      * Two control levels: L2 region, L1 rep.
     FSALESF    IP   F   22        DISK
     FQSYSPRT   O    F  132        PRINTER OFLIND(*INOF)
     DREPTOT           S              9P 2
     DREGTOT           S              9P 2
     DNREC             S              3P 0
     ISALESF    NS  01
     I                                  1    2  REGION        L2
     I                                  3    6  REP           L1
     I                                  7   16  ITEM
     I                             P   17   20 2AMT
     I                             P   21   22 0QTY
     C   L1              Z-ADD     0             NREC
     C   01              ADD       AMT           REPTOT
     C   01              ADD       1             NREC
     CL1                 ADD       REPTOT        REGTOT
     OQSYSPRT   T    L1                     1
     O                       REGION               4
     O                       REP                 10
     O                       NREC          Z     14
     O                       REPTOT        JB    28
     OQSYSPRT   T    L2                     2
     O                       REGION               4
     O                                           10 '**'
     O                       REGTOT        JB    28
     O                                           40 'L1 L2 LR'
     OQSYSPRT   T    LR
     O                                            3 'end'
