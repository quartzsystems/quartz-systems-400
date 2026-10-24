      *; dds: BENCHF
      *; sqlfile: BENCHF
      * A cycle report over 10,000 records with a level break per 100,
      * timed by the program.
     FBENCHF    IP   E           K DISK
     FQSYSPRT   O    F  132        PRINTER OFLIND(*INOF)
     DTOT              S             11P 2
     DGRPS             S              5P 0
     DRECS             S              7P 0
     DT0               S               Z
     DMS               S             20I 0
     IBENCHR        01
     I              BGRP                        BGRP          L1
     C   01              ADD       BAMT          TOT
     C   01              ADD       1             RECS
     CL1                 ADD       1             GRPS
     CLR                 EVAL      MS = %DIFF(%TIMESTAMP():T0:*MSECONDS)
     CLR                 EVAL      MS = MS / 1000
     C     *INZSR        BEGSR
     C                   EVAL      T0 = %TIMESTAMP()
     C                   ENDSR
     OQSYSPRT   T    L1                  1
     O                       BGRP          X     10
     O                       TOT           X     30
     OQSYSPRT   T    LR                  1
     O                                            7 'cycle'
     O                       MS            X     30
     O                       RECS          X     40
     O                       GRPS          X     50
