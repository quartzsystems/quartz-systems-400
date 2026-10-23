     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DC3               S              3A   INZ('abc')
     DC5               S              5A   INZ('abc')
     DZ5               S              5S 2 INZ(1.5)
     DP3               S              3P 0 INZ(2)
     C     C3            COMP      C5                                 414243
     C                   EVAL      LINE = 'a ' + *IN41 + *IN42 + *IN43
     C                   WRITE     QSYSPRT       LINE
     C     C3            COMP      'abd'                              414243
     C                   EVAL      LINE = 'b ' + *IN41 + *IN42 + *IN43
     C                   WRITE     QSYSPRT       LINE
     C     'b'           COMP      C5                                 414243
     C                   EVAL      LINE = 'c ' + *IN41 + *IN42 + *IN43
     C                   WRITE     QSYSPRT       LINE
     C     Z5            COMP      P3                                 414243
     C                   EVAL      LINE = 'd ' + *IN41 + *IN42 + *IN43
     C                   WRITE     QSYSPRT       LINE
     C     Z5            COMP      1.50                               414243
     C                   EVAL      LINE = 'e ' + *IN41 + *IN42 + *IN43
     C                   WRITE     QSYSPRT       LINE
     C     P3            COMP      -5                                 414243
     C                   EVAL      LINE = 'f ' + *IN41 + *IN42 + *IN43
     C                   WRITE     QSYSPRT       LINE
     C     C3            COMP      *BLANKS                            414243
     C                   EVAL      LINE = 'g ' + *IN41 + *IN42 + *IN43
     C                   WRITE     QSYSPRT       LINE
     C     C3            COMP      'abc'                                  44
     C                   EVAL      LINE = 'h ' + *IN44
     C                   WRITE     QSYSPRT       LINE
     C     C3            COMP      'abc'                              4545
     C                   EVAL      LINE = 'i ' + *IN45
     C                   WRITE     QSYSPRT       LINE
     C     'A'           COMP      'a'                                414243
     C                   EVAL      LINE = 'j ' + *IN41 + *IN42 + *IN43
     C                   WRITE     QSYSPRT       LINE
     C     '1'           COMP      'a'                                414243
     C                   EVAL      LINE = 'k ' + *IN41 + *IN42 + *IN43
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
