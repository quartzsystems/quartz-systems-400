     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DB                DS
     DBC                       1      1
     DBU                       1      1U 0
     DMASK             S              1A   INZ(X'0F')
     C                   EVAL      BC = X'00'
     C                   BITON     '0'           BC
     C                   EVAL      LINE = 'a ' + %CHAR(BU)
     C                   WRITE     QSYSPRT       LINE
     C                   BITON     '0167'        BC
     C                   EVAL      LINE = 'b ' + %CHAR(BU)
     C                   WRITE     QSYSPRT       LINE
     C                   BITOFF    '0'           BC
     C                   EVAL      LINE = 'c ' + %CHAR(BU)
     C                   WRITE     QSYSPRT       LINE
     C                   BITON     MASK          BC
     C                   EVAL      LINE = 'd ' + %CHAR(BU)
     C                   WRITE     QSYSPRT       LINE
     C                   BITOFF    X'81'         BC
     C                   EVAL      LINE = 'e ' + %CHAR(BU)
     C                   WRITE     QSYSPRT       LINE
     C                   TESTB     '01'          BC                   414243
     C                   EVAL      LINE = 'f ' + *IN41 + *IN42 + *IN43
     C                   WRITE     QSYSPRT       LINE
     C                   TESTB     '12'          BC                   414243
     C                   EVAL      LINE = 'g ' + *IN41 + *IN42 + *IN43
     C                   WRITE     QSYSPRT       LINE
     C                   TESTB     '67'          BC                   414243
     C                   EVAL      LINE = 'h ' + *IN41 + *IN42 + *IN43
     C                   WRITE     QSYSPRT       LINE
     C                   TESTB     MASK          BC                   414243
     C                   EVAL      LINE = 'i ' + *IN41 + *IN42 + *IN43
     C                   WRITE     QSYSPRT       LINE
     C                   TESTB     X'C0'         BC                   414243
     C                   EVAL      LINE = 'j ' + *IN41 + *IN42 + *IN43
     C                   WRITE     QSYSPRT       LINE
     C                   TESTB     '7'           BC                       44
     C                   EVAL      LINE = 'k ' + *IN44
     C                   WRITE     QSYSPRT       LINE
     C                   TESTB     '0'           BC                   45
     C                   EVAL      LINE = 'l ' + *IN45
     C                   WRITE     QSYSPRT       LINE
     C                   BITON     '01234567'    BC
     C                   EVAL      LINE = 'm ' + %CHAR(BU)
     C                   WRITE     QSYSPRT       LINE
     C                   BITOFF    '01234567'    BC
     C                   EVAL      LINE = 'n ' + %CHAR(BU)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
