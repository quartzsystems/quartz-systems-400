      *; dds: TRANF
      *; sqlfile: TRANF
      * Record identifying indicators from record identification codes.
     FTRANF     IP   F   20        DISK
     FQSYSPRT   O    F  132        PRINTER OFLIND(*INOF)
     ITRANF     AA  11    1 CH
     I                                  2    4 0HCUS
     I                                  5   20  HNAME
     ITRANF     AB  12    1 CD
     I                                  2    8 2DAMT
     I                                  9   20  DITEM
     ITRANF     AC  13
     I                                  1   20  RAW
     C   12              ADD       DAMT          CUSTOT            9 2
     OQSYSPRT   D    11                  1  1
     O                       HCUS                 5
     O                       HNAME               22
     OQSYSPRT   D    12                     1
     O                       DITEM               16
     O                       DAMT          J     28
     O                       CUSTOT        J     42
     OQSYSPRT   D    13                     1
     O                                            6 'other:'
     O                       RAW                 27
