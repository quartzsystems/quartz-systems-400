**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s c char(5) inz('abc');
dcl-s v varchar(5) inz('vv');
dcl-s pk packed(7:2) inz(1.5);
dcl-s zn zoned(6:1) inz(2.5);
dcl-s bn bindec(4:0) inz(7);
dcl-s c2 like(c : +3);
dcl-s c3 like(c : -2);
dcl-s v2 like(v : +5);
dcl-s p2 like(pk : +4);
dcl-s p3 like(pk : -3);
dcl-s z2 like(zn : +1);
dcl-s b2 like(bn : +5);
dcl-s u2 ucs2(3);
dcl-s u3 like(u2 : +2);
line = %char(%len(c2)) + ' ' + %char(%len(c3)) + ' ' + %char(%size(v2)) + ' ' +
  %char(%len(p2)) + ' ' + %char(%decpos(p2)) + ' ' + %char(%len(p3));
write QSYSPRT line;
line = %char(%size(z2)) + ' ' + %char(%len(b2)) + ' ' + %char(%size(b2)) + ' ' +
  %char(%size(u3));
write QSYSPRT line;
*inlr = *on;
