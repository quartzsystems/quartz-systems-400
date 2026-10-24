      *; dds: CUSTF
      *; sqlfile: CUSTF
     HDFTACTGRP(*NO)
     FCUSTF     IF   E           K DISK
     FQSYSPRT   O    F  132        PRINTER
     DCNT              S              3  0
     DBUMP             PR             3  0
     D                                3  0 VALUE
     ICUSTR
     I              CITY                        TOWN
     C                   READ      CUSTR                                  99
     C     *IN99         DOWEQ     *OFF
     C                   EVAL      CNT = BUMP(CNT)
     C                   EXCEPT    LN
     C                   READ      CUSTR                                  99
     C                   ENDDO
     C                   EVAL      *INLR = *ON
     OQSYSPRT   E            LN             1
     O                       CNT           Z      4
     O                       TOWN                20
     PBUMP             B
     D                 PI             3  0
     DN                               3  0 VALUE
     C                   RETURN    N + 1
     PBUMP             E
