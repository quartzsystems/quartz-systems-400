      *; dds: MSTF
      *; sqlfile: MSTF
      *; dds: ACTF
      *; sqlfile: ACTF
      * Primary and secondary without matching: the primary, then the rest.
     FMSTF      IP   F   13        DISK
     FACTF      IS   F    8        DISK
     FQSYSPRT   O    F  132        PRINTER OFLIND(*INOF)
     IMSTF      NS  01
     I                                  1    3  MCUS
     I                                  4   13  MNAME
     IACTF      NS  02
     I                                  1    3  ACUS
     I                                  4    8 0AAMT
     C   02              ADD       AAMT          SUM               7 0
     OQSYSPRT   D    01                     1
     O                                            1 'M'
     O                       MCUS                 5
     O                       MNAME               16
     OQSYSPRT   D    02                     1
     O                                            1 'A'
     O                       ACUS                 5
     O                       AAMT          Z     12
     O                       SUM           Z     20
     OQSYSPRT   T    LR
     O                                            3 'sum'
     O                       SUM           Z     12
