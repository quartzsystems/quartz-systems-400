      *; dds: CUSTF
      *; sqlfile: CUSTF
      * EXSR *PSSR with a return point, and an error at total time.
     FCUSTF     IP   E           K DISK
     FQSYSPRT   O    F  132        PRINTER OFLIND(*INOF)
     Dmsg              S             40
     Dz                S              3P 0 INZ(0)
     Dr                S              5P 0
     Dhit              S              3P 0 INZ(0)
     Dpsds            SDS
     D pstat             *STATUS
     D prtn              *ROUTINE
       msg = 'det ' + %char(CUSNO);
       except;
       if CUSNO = 102;
         exsr *pssr;
         msg = 'after exsr';
         except;
       endif;
     CL0                 EVAL      msg = 'tot ' + %char(CUSNO)
     CL0                 EXCEPT
     CL0                 IF        CUSNO = 105 and hit = 0
     CL0                 EVAL      hit = 1
     CL0                 EVAL      r = 1 / z
     CL0                 ENDIF
       begsr *pssr;
         msg = 'pssr ' + %char(pstat) + ' ' + prtn;
         except;
       endsr '*GETIN';
     OQSYSPRT   D   N1P                     1
     O                                            3 'dl'
     O                       CUSNO         Z     10
     OQSYSPRT   T                           1
     O                                            3 'tl'
     O                       CUSNO         Z     10
     OQSYSPRT   E                           1
     O                       msg                 40
