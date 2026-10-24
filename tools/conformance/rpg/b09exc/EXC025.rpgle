      *; dds: CUSTF
      *; sqlfile: CUSTF
      * A file error in the cycle: no INFSR, ENDSR *GETIN.
     FCUSTF     IP   E           K DISK
     FQSYSPRT   O    F  132        PRINTER OFLIND(*INOF)
     FCUST2     IF   E           K DISK    USROPN EXTDESC('CUSTF')
     F                                     EXTFILE(*EXTDESC) RENAME(CUSTR:C2R)
     F                                     PREFIX(X_) INFDS(fb)
     Dmsg              S             40
     Dz                S              3P 0 INZ(0)
     Dr                S              5P 0
     Dhit              S              3P 0 INZ(0)
     Dpsds            SDS
     D pstat             *STATUS
     D prtn              *ROUTINE
     Dfb               DS
     D fbst              *STATUS
     D fbop              *OPCODE
       msg = 'det ' + %char(CUSNO);
       except;
       if CUSNO <= 102 and hit < 2;
         hit += 1;
         chain (CUSNO) CUST2;
       endif;
       if hit < 0;
         open CUST2;
       endif;
     CL0                 EVAL      msg = 'tot ' + %char(CUSNO)
     CL0                 EXCEPT
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
