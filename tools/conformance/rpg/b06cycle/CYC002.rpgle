      *; dds: SALESF
      *; sqlfile: SALESF
      * An externally described primary file; LR totals.
     FSALESF    IP   E             DISK
     FQSYSPRT   O    F  132        PRINTER OFLIND(*INOF)
     DTOTAL            S              9P 2
     DCOUNT            S              5P 0
     ISALESR        01
     C   01              ADD       AMT           TOTAL
     C   01              ADD       1             COUNT
     CLR                 Z-ADD     TOTAL         AVG               9 2
     CLR   COUNT         IFGT      0
     CLR                 EVAL(H)   AVG = TOTAL / COUNT
     CLR                 ENDIF
     OQSYSPRT   H    1P                     2
     O                                           17 'Region Rep  Item'
     O                                           34 'Amount'
     OQSYSPRT   D    01                     1
     O                       REGION               2
     O                       REP                  9
     O                       ITEM                21
     O                       AMT           J     34
     OQSYSPRT   T    LR                  1
     O                       COUNT         Z      5
     O                                           14 'records'
     O                       TOTAL         J     34
     O                       AVG           J     48
