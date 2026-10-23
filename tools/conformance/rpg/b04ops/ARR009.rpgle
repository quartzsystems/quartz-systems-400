     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DF1               S              5A   INZ('init')
     DF2               S              5P 1 INZ(12.5)
     DF3               S              5A
     DDS1              DS
     DSA                       1      3    INZ('abc')
     DSN                       4      7S 0 INZ(42)
     DARR              S              2A   DIM(3) INZ('aa')
     DMODS             DS                  OCCURS(2)
     DMX                       1      2    INZ('mm')
     DN1               S              1N   INZ(*ON)
     C                   EVAL      F1 = 'chg'
     C                   EVAL      F2 = 1
     C                   EVAL      F3 = 'chg'
     C                   EVAL      SA = 'xyz'
     C                   EVAL      SN = 7
     C                   EVAL      ARR(2) = 'zz'
     C                   EVAL      N1 = *OFF
     C                   RESET                   F1
     C                   RESET                   F2
     C                   RESET                   F3
     C                   EVAL      LINE = 'a [' + F1 + '] ' + %CHAR(F2) + ' [' +
     C                             F3 + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   RESET                   SN
     C                   EVAL      LINE = 'b ' + SA + ' ' + %CHAR(SN)
     C                   WRITE     QSYSPRT       LINE
     C                   RESET                   DS1
     C                   EVAL      LINE = 'c ' + SA + ' ' + %CHAR(SN)
     C                   WRITE     QSYSPRT       LINE
     C                   RESET                   ARR
     C                   RESET                   N1
     C                   EVAL      LINE = 'd ' + ARR(2) + ' ' + N1
     C                   WRITE     QSYSPRT       LINE
     C                   CLEAR                   DS1
     C                   CLEAR                   ARR
     C                   CLEAR                   F2
     C                   EVAL      LINE = 'e [' + SA + '] ' + %CHAR(SN) + ' [' +
     C                             ARR(1) + '] ' + %CHAR(F2)
     C                   WRITE     QSYSPRT       LINE
     C                   CLEAR                   N1
     C                   EVAL      LINE = 'f ' + N1
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      MX = 'p1'
     C     2             OCCUR     MODS
     C                   EVAL      MX = 'p2'
     C                   RESET                   MODS
     C                   EVAL      LINE = 'g ' + MX
     C                   WRITE     QSYSPRT       LINE
     C     1             OCCUR     MODS
     C                   EVAL      LINE = 'h ' + MX
     C                   WRITE     QSYSPRT       LINE
     C                   CLEAR     *ALL          MODS
     C     2             OCCUR     MODS
     C                   EVAL      LINE = 'i [' + MX + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   RESET     *ALL          MODS
     C     1             OCCUR     MODS
     C                   EVAL      LINE = 'j [' + MX + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *IN05 = *ON
     C                   CLEAR                   *IN
     C                   EVAL      LINE = 'k ' + *IN05
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
     C     *INZSR        BEGSR
     C                   EVAL      F1 = 'inzsr'
     C                   EVAL      SN = 99
     C                   ENDSR
