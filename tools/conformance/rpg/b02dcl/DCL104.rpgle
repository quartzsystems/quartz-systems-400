**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s c1 char(4) inz(*blanks);
dcl-s c2 char(4) inz(*zeros);
dcl-s v1 varchar(4) inz(*blanks);
dcl-s v2 varchar(4) inz(*zeros);
dcl-s p1 packed(5:2) inz(*zeros);
dcl-s z1 zoned(3:0) inz(*zeros);
dcl-s i1 int(10) inz(*zeros);
dcl-s f1 float(8) inz(*zeros);
dcl-s u1 ucs2(3) inz(*blanks);
dcl-ds ds1 inz;
  a char(2);
  b packed(3:0);
end-ds;
line = '[' + c1 + '][' + c2 + '][' + v1 + '][' + v2 + '] ' + %char(%len(v1)) +
  %char(%len(v2));
write QSYSPRT line;
line = %char(p1) + ' ' + %char(z1) + ' ' + %char(i1) + ' ' + %char(f1);
write QSYSPRT line;
line = '[' + %char(u1) + '] ' + a + ' ' + %char(b);
write QSYSPRT line;
*inlr = *on;
