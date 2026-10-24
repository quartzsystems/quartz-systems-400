      *; dds: MSTF
      *; sqlfile: MSTF
      *; dds: ACTF
      *; sqlfile: ACTF
     FMSTF      IP   F   13        DISK
     FACTF      IS   F    8        DISK
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     IMSTF      AA  01
     I                                  1    3  MCUS
     I                                  4   13  MNAME
     IACTF      BB  02
     I                                  1    3  ACUS
     I                                  4    8 0AAMT
     C   01              EVAL      LINE = 'master ' + MCUS + ' ' + MNAME
     C   01              WRITE     QSYSPRT       LINE
     C   01              FORCE     ACTF
     C   02              EVAL      LINE = 'activity ' + ACUS
     C   02              WRITE     QSYSPRT       LINE
