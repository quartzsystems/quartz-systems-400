     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DCH               S              1    INZ(X'00')
     DDG               S              3    INZ('123')
     DBL               S              3    INZ(' 12')
     C                   BITON     '01'          CH
     C                   TESTB     '0'           CH                   21  23
     C                   EVAL      LINE = 'b0 ' + *IN21 + *IN22 + *IN23
     C                   WRITE     QSYSPRT       LINE
     C                   TESTB     '01'          CH                   212223
     C                   EVAL      LINE = 'b01 ' + *IN21 + *IN22 + *IN23
     C                   WRITE     QSYSPRT       LINE
     C                   TESTB     '012'         CH                   212223
     C                   EVAL      LINE = 'b012 ' + *IN21 + *IN22 + *IN23
     C                   WRITE     QSYSPRT       LINE
     C                   TESTN                   DG                   313233
     C                   EVAL      LINE = 'n ' + *IN31 + *IN32 + *IN33
     C                   WRITE     QSYSPRT       LINE
     C                   TESTN                   BL                   313233
     C                   EVAL      LINE = 'nb ' + *IN31 + *IN32 + *IN33
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
