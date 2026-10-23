      *; dds: SALESF
      *; sqlfile: SALESF
      * Overflow: headings conditioned by 1P or OF, page numbers.
     FSALESF    IP   F   22        DISK
     FQSYSPRT   O    F  132        PRINTER OFLIND(*INOF)
     ISALESF    NS  01
     I                                  1    2  REGION
     I                                  3    6  REP
     I                                  7   16  ITEM
     I                             P   17   20 2AMT
     I                             P   21   22 0QTY
     OQSYSPRT   H    1P                     2  1
     O         OR    OF
     O                                           13 'Sales by line'
     O                                           40 'Page'
     O                       PAGE          Z     45
     OQSYSPRT   D    01                     3
     O                       REGION               4
     O                       ITEM                16
     O                       AMT           J     28
     OQSYSPRT   D    01                     3
     O                                            9 'again'
     O                       REP                 16
     OQSYSPRT   T    LR                  1
     O                                            9 'Last page'
     O                       PAGE          Z     15
