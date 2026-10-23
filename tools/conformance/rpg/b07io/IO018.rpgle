      *; dds: CUSTF
      *; sqlfile: CUSTF
      * Fixed form: SETLL and SETGT indicators, READP, CHAIN (N).
     FCUSTF     UF   E           K DISK
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DKEY              S              5P 0
     C                   Z-ADD     104           KEY
     C     KEY           SETLL     CUSTF                              60  61
     C                   EVAL      LINE = 'll 104 ' + *IN60 + *IN61
     C                   WRITE     QSYSPRT       LINE
     C                   Z-ADD     109           KEY
     C     KEY           SETLL     CUSTF                              60  61
     C                   EVAL      LINE = 'll 109 ' + *IN60 + *IN61
     C                   WRITE     QSYSPRT       LINE
     C                   Z-ADD     103           KEY
     C     KEY           SETGT     CUSTR                              62
     C                   READ      CUSTF                                  63
     C                   EVAL      LINE = 'gt 103 ' + *IN62 + ' ' + %CHAR(CUSNO)
     C                             + *IN63
     C                   WRITE     QSYSPRT       LINE
     C                   READP     CUSTF                                  63
     C                   EVAL      LINE = 'readp ' + %CHAR(CUSNO) + *IN63
     C                   WRITE     QSYSPRT       LINE
     C                   Z-ADD     105           KEY
     C     KEY           CHAIN(N)  CUSTF                              64
     C                   EVAL      LINE = 'chain ' + %CHAR(CUSNO) + ' ' + NAME +
     C                             *IN64
     C                   WRITE     QSYSPRT       LINE
     C     KEY           CHAIN     CUSTF                              64
     C                   EVAL      BAL = BAL * 2
     C                   UPDATE    CUSTR                                65
     C     KEY           CHAIN(N)  CUSTF                              64
     C                   EVAL      LINE = 'upd ' + %CHAR(BAL) + ' ' + *IN65
     C                   WRITE     QSYSPRT       LINE
     C                   UPDATE    CUSTR                                65
     C                   EVAL      LINE = 'upd again ' + *IN65 + ' ' +
     C                             %CHAR(%STATUS(CUSTF))
     C                   WRITE     QSYSPRT       LINE
     C                   SETON                                        LR
