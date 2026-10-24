     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DREC              DS
      * the first subfield
     D A                              3    INZ('aaa')                           trailing comment

     D B                              2    INZ('bb')
       // a free-form comment between subfields
     D C                              4S 0 INZ(12)
     C                   EVAL      LINE = REC + ' ' + %CHAR(%SIZE(REC))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
