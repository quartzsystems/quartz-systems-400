     FQSYSPRT   O    F  132        PRINTER
       dcl-ds line len(132) end-ds;
     DA                S              3    INZ('fx1')
       dcl-s b char(3) inz('fr1');
     DDS1              DS
     D C                              2    INZ('cc')
       dcl-ds ds2;
         e char(2) inz('ee');
       end-ds;
     DF                S                   LIKE(B) INZ('fx2')
     C                   EVAL      LINE = A + B + DS1 + DS2 + F
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
