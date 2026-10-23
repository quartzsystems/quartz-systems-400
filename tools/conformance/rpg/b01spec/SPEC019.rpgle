**FREE
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s a int(10) inz(1); dcl-s b int(10) inz(2);
dcl-s c
      packed(9:2)
      inz(10.5);
c = c
    + a
    * b;
line = 'c=' +
       %char(c); write QSYSPRT line;
if a < b and
   c > 0;
  line = 'yes'; write QSYSPRT line;
endif;
*inlr = *on;
