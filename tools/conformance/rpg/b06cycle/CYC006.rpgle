      *; dds: CUSTF
      *; sqlfile: CUSTF
      * 1P output, detail calculations and a halt on LR set early.
     FCUSTF     IP   E           K DISK
     FQSYSPRT   O    F  132        PRINTER OFLIND(*INOF)
     DN                S              3P 0
     C                   ADD       1             N
     C     N             IFEQ      5
     C                   SETON                                        LR
     C                   ENDIF
     OQSYSPRT   H    1P                     1
     O                                           15 'First page only'
     OQSYSPRT   D   N1P                     1
     O                       N             Z      3
     O                       CUSNO         Z     10
     O                       NAME                32
     O                       BAL           K     46
     OQSYSPRT   T    LR
     O                                           14 'Stopped after'
     O                       N             Z     18
