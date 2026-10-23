**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s d date inz(d'2026-03-05');
dcl-s e date(*mdy) inz(d'1999-12-31');
line = %char(d:*iso) + ' ' + %char(d:*usa) + ' ' + %char(d:*eur) + ' ' + %char(d:*jis);
write QSYSPRT line;
line = %char(d:*mdy) + ' ' + %char(d:*dmy) + ' ' + %char(d:*ymd) + ' ' + %char(d:*jul);
write QSYSPRT line;
line = %char(d:*iso0) + ' ' + %char(d:*usa0) + ' ' + %char(d:*eur0) + ' ' +
      %char(d:*jis0);
write QSYSPRT line;
line = %char(d:*mdy0) + ' ' + %char(d:*dmy0) + ' ' + %char(d:*ymd0) + ' ' +
      %char(d:*jul0);
write QSYSPRT line;
line = %char(d:*cymd) + ' ' + %char(d:*cmdy) + ' ' + %char(d:*cdmy) + ' ' +
      %char(d:*longjul);
write QSYSPRT line;
line = %char(d:*cymd0) + ' ' + %char(d:*cmdy0) + ' ' + %char(d:*cdmy0) + ' ' +
      %char(d:*longjul0);
write QSYSPRT line;
line = %char(d:*mdy-) + ' ' + %char(d:*dmy.) + ' ' + %char(d:*ymd,) + ' ' + %char(d:*jul&)
      + '|';
write QSYSPRT line;
line = %char(e:*cymd) + ' ' + %char(e) + ' ' + %char(e:*iso) + ' ' + %char(e:*cmdy-);
write QSYSPRT line;
line = '[' + %char(d) + '] ' + %char(%len(%char(e:*cymd)));
write QSYSPRT line;
*inlr = *on;
