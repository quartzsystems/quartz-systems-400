**FREE
//; run: no
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s arr char(3) dim(5);
dcl-s tab char(2) dim(3) ctdata;
dcl-ds plain;
  f1 char(4);
  f2 packed(5:0);
  whole char(7) overlay(plain);
end-ds;
dcl-ds tmpl qualified template;
  id int(10);
  nm char(8);
end-ds;
dcl-ds rec likeds(tmpl) dim(2);
dcl-s buf char(20);
dcl-s p pointer inz(%addr(buf));
dcl-s view char(5) based(p);
arr(2) = 'b';
f1 = 'abcd';
rec(1).id = 7;
rec(2).nm = 'two';
line = arr(2) + whole + %char(rec(1).id) + rec(2).nm + view + tab(1);
write QSYSPRT line;
*inlr = *on;
**CTDATA tab
t1
t2
t3
