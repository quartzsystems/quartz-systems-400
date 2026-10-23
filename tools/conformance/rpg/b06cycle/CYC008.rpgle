      *; dds: MSTF
      *; sqlfile: MSTF
      *; dds: ACTF
      *; sqlfile: ACTF
      * Matching records: masters primary, activity secondary, M1, MR.
     FMSTF      IP   F   13        DISK
     FACTF      IS   F    8        DISK
     FQSYSPRT   O    F  132        PRINTER OFLIND(*INOF)
     IMSTF      AA  01
     I                                  1    3  MCUS            M1
     I                                  4   13  MNAME
     IACTF      BB  02
     I                                  1    3  ACUS            M1
     I                                  4    8 0AAMT
     OQSYSPRT   D    01 MR                  1
     O                       MCUS                 3
     O                       MNAME               15
     O                                           37 'master with activity'
     OQSYSPRT   D    01NMR                  1
     O                       MCUS                 3
     O                       MNAME               15
     O                                           29 'master alone'
     OQSYSPRT   D    02 MR                  1
     O                       ACUS                 7
     O                       AAMT          Z     15
     O                                           24 'matched'
     OQSYSPRT   D    02NMR                  1
     O                       ACUS                 7
     O                       AAMT          Z     15
     O                                           26 'unmatched'
