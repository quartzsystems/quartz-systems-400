     HDFTACTGRP(*NO) DATFMT(*YMD)
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DCHR10            S             10    INZ('2026-03-05')
     DCHR8             S              8    INZ('26/02/30')
     DCHR6             S              6    INZ('260228')
     DNUM6             S              6S 0 INZ(022826)
     DNUM7             S              7P 0 INZ(1260229)
     DTCHR             S              8    INZ('14:30:61')
     DZCHR             S             26    INZ('2026-03-05-14.30.05.123456')
     DBAD              DS
     DBADDATE                          D   DATFMT(*ISO)
     DBADCHR                         10    OVERLAY(BAD)
     C     *ISO          TEST(D)                 CHR10                  50
     C                   EVAL      LINE = 'iso ' + *IN50
     C                   WRITE     QSYSPRT       LINE
     C                   TEST(D)                 CHR8                   51
     C                   EVAL      LINE = 'ymd ' + *IN51
     C                   WRITE     QSYSPRT       LINE
     C     *YMD0         TEST(D)                 CHR6                   52
     C                   EVAL      LINE = 'ymd0 ' + *IN52
     C                   WRITE     QSYSPRT       LINE
     C     *MDY          TEST(D)                 NUM6                   53
     C                   EVAL      LINE = 'mdy num ' + *IN53
     C                   WRITE     QSYSPRT       LINE
     C     *CYMD         TEST(D)                 NUM7                   54
     C                   EVAL      LINE = 'cymd num ' + *IN54
     C                   WRITE     QSYSPRT       LINE
     C     *HMS          TEST(T)                 TCHR                   55
     C                   EVAL      LINE = 'hms ' + *IN55
     C                   WRITE     QSYSPRT       LINE
     C                   TEST(Z)                 ZCHR                   56
     C                   EVAL      LINE = 'stamp ' + *IN56
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      BADCHR = '2026-13-01'
     C                   TEST                    BADDATE                57
     C                   EVAL      LINE = 'field ' + *IN57
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      BADCHR = '2026-12-01'
     C                   TEST                    BADDATE                58
     C                   EVAL      LINE = 'field ' + *IN58
     C                   WRITE     QSYSPRT       LINE
     C                   SETON                                        LR
