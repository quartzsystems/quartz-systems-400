     HDFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DAL               DS                  ALIGN
     D C1                             1
     D I4                            10I 0
     D C2                             1
     D I8                            20I 0
     DPE               DS
     D PK                      1      2P 0 PACKEVEN
     D PO                      3      4P 0
     DSP               DS                  QUALIFIED
     D A                              4
     D B                              6
     D C                              4    SAMEPOS(B)
     C                   EVAL      PK = 99
     C                   EVAL      PO = 999
     C                   EVAL      SP.B = 'abcdef'
     C                   EVAL      LINE = %CHAR(%SIZE(AL)) + ' ' +
     C                             %CHAR(%LEN(PK)) + ' ' + %CHAR(%LEN(PO)) + ' '
     C                             + %CHAR(PK) + ' ' + SP.C
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
