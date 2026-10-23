      *; dds: SALESF
      *; sqlfile: SALESF
      * A primary file and detail output only: no calculations.
     FSALESF    IP   F   22        DISK
     FQSYSPRT   O    F  132        PRINTER OFLIND(*INOF)
     ISALESF    NS  01
     I                                  1    2  REGION
     I                                  3    6  REP
     I                                  7   16  ITEM
     I                             P   17   20 2AMT
     I                             P   21   22 0QTY
     OQSYSPRT   D    01                     1
     O                       REGION               4
     O                       REP                 10
     O                       ITEM                22
     O                       AMT           2     34
     O                       QTY           Z     40
