     HDFTACTGRP(*NO) THREAD(*SERIALIZE)
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DSTR              S               O   CLASS(*JAVA:'java.lang.String')
     C                   IF        STR = *NULL
     C                   EVAL      LINE = 'class null'
     C                   WRITE     QSYSPRT       LINE
     C                   ENDIF
     C                   EVAL      *INLR = *ON
