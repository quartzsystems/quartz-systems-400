      *; dds: CUSTF
      *; sqlfile: CUSTF
      * Fixed form: WRITE and DELETE with indicators, an INFDS.
     FCUSTF     UF A E           K DISK    INFDS(FB)
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DKEY              S              5P 0
     DFB               DS
     DFSTAT              *STATUS
     DFOP                *OPCODE
     C                   EVAL      CUSNO = 102
     C                   WRITE     CUSTR                                70
     C                   EVAL      LINE = 'dup ' + *IN70 + ' ' + %CHAR(FSTAT) +
     C                             ' ' + FOP
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      CUSNO = 130
     C                   EVAL      NAME = 'New One'
     C                   WRITE     CUSTR                                70
     C                   EVAL      LINE = 'new ' + *IN70 + ' ' + %CHAR(FSTAT) +
     C                             ' ' + FOP
     C                   WRITE     QSYSPRT       LINE
     C                   Z-ADD     130           KEY
     C     KEY           DELETE    CUSTR                              7172
     C                   EVAL      LINE = 'del ' + *IN71 + *IN72 + ' ' + FOP
     C                   WRITE     QSYSPRT       LINE
     C                   Z-ADD     131           KEY
     C     KEY           DELETE    CUSTF                              7172
     C                   EVAL      LINE = 'del ' + *IN71 + *IN72 + ' ' + FOP
     C                   WRITE     QSYSPRT       LINE
     C     *LOVAL        SETLL     CUSTF
     C                   READ      CUSTF                                  99
     C     *IN99         DOWEQ     *OFF
     C                   EVAL      LINE = %CHAR(CUSNO) + ' ' + NAME
     C                   WRITE     QSYSPRT       LINE
     C                   READ      CUSTF                                  99
     C                   ENDDO
     C                   SETON                                        LR
