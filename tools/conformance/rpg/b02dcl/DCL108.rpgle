**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s a char(8) inz('it''s');
dcl-s b char(4) inz(x'C1C2C3C4');
dcl-s c char(7) inz(*all'ab');
dcl-s d char(5) inz(*allx'F1');
dcl-s e varchar(10) inz('');
dcl-s f ucs2(4) inz(u'00410042');
dcl-s g varchar(6) inz(*all'xy');
dcl-s h char(3) inz('');
dcl-s i ucs2(3) inz(u'006100620063');
line = '[' + a + '][' + b + '][' + c + '][' + d + '][' + e + '] ' + %char(%len(e));
write QSYSPRT line;
line = '[' + %char(f) + '][' + g + '] ' + %char(%len(g)) + ' [' + h + '] ' + %char(i);
write QSYSPRT line;
*inlr = *on;
