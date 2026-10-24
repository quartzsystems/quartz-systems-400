      *; dds: CUSTF
      *; sqlfile: CUSTF
      * A cycle program's *PSSR returning to *TOTC: the total calculations.
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
       if CUSNO = 103 and hit = 0;
         hit = 1;
         r = 1 / z;
       endif;
     CL0                 EVAL      msg = 'tot ' + %char(CUSNO)
     CL0                 EXCEPT
       begsr *pssr;
         msg = 'pssr ' + %char(pstat) + ' ' + prtn;
         except;
       endsr '*TOTC';
     OQSYSPRT   D   N1P                     1
     O                                            3 'dl'
     O                       CUSNO         Z     10
     OQSYSPRT   T                           1
     O                                            3 'tl'
     O                       CUSNO         Z     10
     OQSYSPRT   E                           1
     O                       msg                 40
