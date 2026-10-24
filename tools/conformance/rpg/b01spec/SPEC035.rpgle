      *; dds: CUSTF
      *; sqlfile: CUSTF
     FCUSTF     IF   E           K DISK
      * the prefix comes next
     F                                     PREFIX(P_)

     F                                     INFDS(FB)
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DFB               DS
     D FNAME             *FILE
     D FSTAT             *STATUS
     C                   READ      CUSTR
     C                   EVAL      LINE = FNAME + ' ' + %CHAR(FSTAT) + ' ' +
     C                             %TRIM(P_NAME)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
