      *; dds: CUSTF
      *; sqlfile: CUSTF
     FCUSTF     IF   E           K DISK    USROPN
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     C                   EVAL      *IN10 = %OPEN(CUSTF)
     C                   EVAL      LINE = 'before ' + *IN10
     C                   WRITE     QSYSPRT       LINE
     C                   OPEN      CUSTF
     C                   EVAL      *IN10 = %OPEN(CUSTF)
     C                   READ      CUSTR
     C                   EVAL      LINE = 'open ' + *IN10 + ' ' + %TRIM(NAME)
     C                   WRITE     QSYSPRT       LINE
     C                   CLOSE     CUSTF
     C                   EVAL      *IN10 = %OPEN(CUSTF)
     C                   EVAL      LINE = 'after ' + *IN10
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
