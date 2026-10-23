      *; dds: SALESF
      *; sqlfile: SALESF
      * A classic report: headings, details, rep and region totals.
     FSALESF    IP   E             DISK
     FQSYSPRT   O    F  132        PRINTER OFLIND(*INOF)
     DEXT              S              9P 2
     DREPTOT           S              9P 2
     DREGTOT           S             11P 2
     DGRAND            S             11P 2
     DLINES            S              3P 0
     ISALESR        01
     I              REGION                      REGION        L2
     I              REP                         REP           L1
     C   01AMT           MULT      QTY           EXT
     C   01              ADD       EXT           REPTOT
     C   01              ADD       1             LINES
     CL1                 ADD       REPTOT        REGTOT
     CL2                 ADD       REGTOT        GRAND
     OQSYSPRT   H    1P                     1  1
     O         OR    OF
     O                                           23 'Sales by region and rep'
     O                                           56 'Page'
     O                       PAGE          Z     61
     OQSYSPRT   H    1P                     2
     O         OR    OF
     O                                           14 'Reg Rep  Item'
     O                                           30 'Qty'
     O                                           42 'Price'
     O                                           56 'Extended'
     OQSYSPRT   D    01 L1                  1
     O                       REGION               3
     O                       REP                  8
     OQSYSPRT   D    01                     1
     O                       ITEM                20
     O                       QTY           3     30
     O                       AMT           1     42
     O                       EXT           1     56
     OQSYSPRT   T    L1                     1
     O                                           14 'Rep'
     O                       REP                 19
     O                                           25 'total'
     O                       REPTOT        1B    56
     O                                           57 '*'
     OQSYSPRT   T    L2                     2
     O                                           11 'Region'
     O                       REGION              14
     O                                           20 'total'
     O                       REGTOT        1B    56
     O                                           58 '**'
     OQSYSPRT   T    LR                  1
     O                                           11 'Grand total'
     O                       LINES         Z     20
     O                                           26 'lines'
     O                       GRAND         1     56
     O                                           59 '***'
