      *; dds: BENCHF
      *; sqlfile: BENCHF
      * *INZSR in a cycle program: it runs once, and the detail
      * calculations before it go on to the next record.
     FBENCHF    IP   E           K DISK
     FQSYSPRT   O    F  132        PRINTER OFLIND(*INOF)
     DTOT              S             11P 2
     DGRPS             S              5P 0
     DRECS             S              7P 0
     IBENCHR        01
     I              BGRP                        BGRP          L1
     C   01              ADD       BAMT          TOT
     C   01              ADD       1             RECS
     CL1                 ADD       1             GRPS
     C     *INZSR        BEGSR
     C                   Z-ADD     1000          GRPS
     C                   ENDSR
     OQSYSPRT   T    L1                  1
     O                       BGRP          X     10
     O                       TOT           X     30
     OQSYSPRT   T    LR                  1
     O                                            7 'total'
     O                       RECS          X     40
     O                       GRPS          X     50
