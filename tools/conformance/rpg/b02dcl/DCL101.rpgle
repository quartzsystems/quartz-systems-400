**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds ds1 inz;
  d1 date(*mdy);
  d2 date(*iso);
  d3 date(*jul);
  t1 time(*usa);
  z1 timestamp;
end-ds;
dcl-s s1 date(*ymd);
dcl-s s2 time;
line = %char(d1) + ' ' + %char(d2) + ' ' + %char(d3) + ' ' + %char(t1);
write QSYSPRT line;
line = %char(z1) + ' ' + %char(s1) + ' ' + %char(s2);
write QSYSPRT line;
*inlr = *on;
