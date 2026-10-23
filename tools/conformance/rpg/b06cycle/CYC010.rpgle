      *; dds: CUSTF
      *; sqlfile: CUSTF
      * Field indicators: plus, minus and zero on an external field.
     FCUSTF     IP   E           K DISK
     FQSYSPRT   O    F  132        PRINTER OFLIND(*INOF)
     ICUSTR         01
     I              BAL                         BAL                 313233
     I              CITY                        CITY          L1
     OQSYSPRT   D    01 31                  1
     O                       CUSNO                5
     O                                           13 'credit'
     OQSYSPRT   D    01 32                  1
     O                       CUSNO                5
     O                                           12 'debit'
     OQSYSPRT   D    01 33                  1
     O                       CUSNO                5
     O                                           11 'zero'
     OQSYSPRT   D    L1                     1
     O                                           10 'city break'
     O                       CITY                23
