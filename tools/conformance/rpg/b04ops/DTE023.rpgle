     HDFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DDATE1            S               D   DATFMT(*USA) INZ(D'2026-03-05')
     DDATE2            S               D
     DDATE3            S               D   DATFMT(*YMD)
     DTIME1            S               T   TIMFMT(*HMS) INZ(T'14.30.05')
     DTIME2            S               T
     DSTAMP1           S               Z   INZ(Z'2026-03-05-14.30.05.123456')
     DSTAMP2           S               Z
     DNUM              S              9P 0
     DNDAYS            S              5P 0 INZ(10)
     C     DATE1         ADDDUR    5:*D          DATE2
     C                   EVAL      LINE = %CHAR(DATE2)
     C                   WRITE     QSYSPRT       LINE
     C                   ADDDUR    1:*M          DATE1
     C                   EVAL      LINE = %CHAR(DATE1)
     C                   WRITE     QSYSPRT       LINE
     C     DATE1         SUBDUR    DATE2         NUM:*D
     C                   EVAL      LINE = %CHAR(NUM)
     C                   WRITE     QSYSPRT       LINE
     C     DATE2         SUBDUR    NDAYS:*DAYS   DATE3
     C                   EVAL      LINE = %CHAR(DATE3)
     C                   WRITE     QSYSPRT       LINE
     C                   SUBDUR    2:*Y          DATE3
     C                   EVAL      LINE = %CHAR(DATE3)
     C                   WRITE     QSYSPRT       LINE
     C     DATE1         SUBDUR    DATE3         NUM:*M
     C                   EVAL      LINE = %CHAR(NUM)
     C                   WRITE     QSYSPRT       LINE
     C     TIME1         ADDDUR    90:*MN        TIME2
     C                   EVAL      LINE = %CHAR(TIME2)
     C                   WRITE     QSYSPRT       LINE
     C     TIME2         SUBDUR    TIME1         NUM:*S
     C                   EVAL      LINE = %CHAR(NUM)
     C                   WRITE     QSYSPRT       LINE
     C     STAMP1        ADDDUR    1:*MS         STAMP2
     C                   EVAL      LINE = %CHAR(STAMP2)
     C                   WRITE     QSYSPRT       LINE
     C                   ADDDUR    -36:*H        STAMP2
     C                   EVAL      LINE = %CHAR(STAMP2)
     C                   WRITE     QSYSPRT       LINE
     C     STAMP1        SUBDUR    STAMP2        NUM:*H
     C                   EVAL      LINE = %CHAR(NUM)
     C                   WRITE     QSYSPRT       LINE
     C                   EXTRCT    DATE1:*Y      NUM
     C                   EVAL      LINE = %CHAR(NUM)
     C                   WRITE     QSYSPRT       LINE
     C                   EXTRCT    DATE1:*M      NUM
     C                   EVAL      LINE = %CHAR(NUM)
     C                   WRITE     QSYSPRT       LINE
     C                   EXTRCT    TIME1:*MN     NUM
     C                   EVAL      LINE = %CHAR(NUM)
     C                   WRITE     QSYSPRT       LINE
     C                   EXTRCT    STAMP1:*MS    NUM
     C                   EVAL      LINE = %CHAR(NUM)
     C                   WRITE     QSYSPRT       LINE
     C                   SETON                                        LR
