      *; dds: HISTF
      *; sqlfile: HISTF
      * Program-described output to a disk file: O specs with ADD.
     FHISTF     O    F   22        DISK    BLOCK(*NO)
     FHISTIN    IF   E             DISK    EXTFILE('HISTF') EXTDESC('HISTF')
     F                                     RENAME(HISTR:HIN) PREFIX(I)
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DNUM              S              5P 0
     DTXT              S             15
     DAMT              S              7P 2
     C                   EVAL      NUM = 201
     C                   EVAL      TXT = 'Added one'
     C                   EVAL      AMT = 12.5
     C                   EXCEPT    NEWREC
     C                   EVAL      NUM = 202
     C                   EVAL      AMT = -1
     C                   EXCEPT    NEWREC
     C                   READ      HISTIN                                 99
     C     *IN99         DOWEQ     *OFF
     C                   EVAL      LINE = %CHAR(IHCUS) + ' ' + IHTEXT +
     C                             %CHAR(IHAMT)
     C                   WRITE     QSYSPRT       LINE
     C                   READ      HISTIN                                 99
     C                   ENDDO
     C                   SETON                                        LR
     OHISTF     E            NEWREC
     O                       NUM                  3P
     O                       TXT                 18
     O                       AMT                 22P
