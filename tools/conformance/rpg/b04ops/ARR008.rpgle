     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DMODS             DS                  OCCURS(3)
     DMA                       1      3
     DMN                       4      6S 0
     DOTHER            DS                  OCCURS(3)
     DOA                       1      2
     DN                S              3P 0
     C                   EVAL      MA = 'one'
     C                   EVAL      MN = 1
     C     2             OCCUR     MODS
     C                   EVAL      MA = 'two'
     C                   EVAL      MN = 2
     C     3             OCCUR     MODS
     C                   EVAL      MA = 'thr'
     C                   EVAL      MN = 3
     C     2             OCCUR     MODS          N
     C                   EVAL      LINE = 'a ' + MA + ' ' + %CHAR(MN) + ' ' +
     C                             %CHAR(N)
     C                   WRITE     QSYSPRT       LINE
     C     1             OCCUR     MODS
     C                   EVAL      LINE = 'b ' + MA + ' ' + %CHAR(MN) + ' ' +
     C                             %CHAR(%OCCUR(MODS))
     C                   WRITE     QSYSPRT       LINE
     C                   OCCUR     MODS          N
     C                   EVAL      LINE = 'c ' + %CHAR(N)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      N = 4
     C     N             OCCUR     MODS                                 40
     C                   EVAL      LINE = 'd ' + *IN40 + ' ' + MA + ' ' +
     C                             %CHAR(%OCCUR(MODS))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      %OCCUR(MODS) = 3
     C                   EVAL      LINE = 'e ' + MA + ' ' + %CHAR(MN)
     C                   WRITE     QSYSPRT       LINE
     C     MODS          OCCUR     OTHER
     C                   EVAL      LINE = 'f ' + %CHAR(%OCCUR(OTHER)) + ' [' +
     C                             OA + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      OA = 'zz'
     C     1             OCCUR     OTHER
     C                   EVAL      LINE = 'g [' + OA + ']'
     C                   WRITE     QSYSPRT       LINE
     C     3             OCCUR     OTHER
     C                   EVAL      LINE = 'h [' + OA + ']'
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      N = 2
     C     N             OCCUR     MODS
     C                   EVAL      LINE = 'i ' + MA
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
