     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DTABA             S              3    DIM(4) CTDATA PERRCD(1)
     DTABB             S              5    DIM(4) ALT(TABA)
     DTABN             S              3S 0 DIM(4) CTDATA PERRCD(2)
     DX                S              3A
     DK3               S              3S 0 INZ(20)
     C                   EVAL      X = TABA
     C                   EVAL      LINE = 'a [' + X + ']'
     C                   WRITE     QSYSPRT       LINE
     C     'CCC'         LOOKUP    TABA          TABB                     50
     C                   EVAL      LINE = 'b ' + *IN50 + ' ' + TABA + ' ' + TABB
     C                   WRITE     QSYSPRT       LINE
     C     'ZZZ'         LOOKUP    TABA          TABB                     50
     C                   EVAL      LINE = 'c ' + *IN50 + ' ' + TABA + ' ' + TABB
     C                   WRITE     QSYSPRT       LINE
     C     'AAA'         LOOKUP    TABA                                   50
     C                   EVAL      LINE = 'd ' + *IN50 + ' ' + TABA + ' ' + TABB
     C                   WRITE     QSYSPRT       LINE
     C     'BBB'         LOOKUP    TABA          TABB                     50
     C                   EVAL      TABB = 'new'
     C     'BBB'         LOOKUP    TABA          TABB                     50
     C                   EVAL      LINE = 'e ' + *IN50 + ' ' + TABA + ' ' + TABB
     C                   WRITE     QSYSPRT       LINE
     C     K3            LOOKUP    TABN                                   51
     C                   EVAL      LINE = 'f ' + *IN51 + ' ' + %CHAR(TABN)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
**CTDATA TABA
AAAone
BBBtwo
CCCthree
DDDfour
**CTDATA TABN
010020
030040
