      *; dds: SALESF
      *; sqlfile: SALESF
      *; run: no
      * Control levels L1 and L2 on a program-described file.
     FSALESF    IP   F   22        DISK
     FQSYSPRT   O    F  132        PRINTER
     DTOT              S              9P 2
     ISALESF    NS  01
     I                                  1    2  REGION        L2
     I                                  3    6  REP           L1
     I                             P   17   20 2AMT
     C  01               ADD       AMT           TOT
     CL1                 Z-ADD     0             TOT
     OQSYSPRT   T    L1                     1
     O                       REGION               4
     O                       REP                 10
     O                       TOT           JB    25
