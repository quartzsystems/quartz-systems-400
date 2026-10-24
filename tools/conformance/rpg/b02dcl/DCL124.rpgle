**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s a1 zoned(3:1) dim(2) inz(1.5);
dcl-s a2 bindec(4:0) dim(2) inz(-9);
dcl-s a3 uns(3) dim(2) inz(255);
dcl-s a4 float(4) dim(2) inz(0.5);
dcl-s a5 ind dim(2) inz(*on);
dcl-s a6 date(*dmy) dim(2) inz(d'2026-12-01');
dcl-s a7 time(*hms) dim(2) inz(t'23.00.01');
dcl-s a8 timestamp dim(2) inz(z'2026-12-01-23.00.01.000000');
dcl-s a9 pointer dim(2);
line = %char(a1(2)) + ' ' + %char(a2(2)) + ' ' + %char(a3(2)) + ' ' + %char(a4(2)) + ' ' +
  a5(2);
write QSYSPRT line;
line = %char(a6(2)) + ' ' + %char(a7(2)) + ' ' + %char(a8(2)) + ' ' + %char(%size(a9 :
  *all));
write QSYSPRT line;
*inlr = *on;
