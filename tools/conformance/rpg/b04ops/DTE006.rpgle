**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s d date inz(d'2026-03-05');
dcl-s e date(*mdy) inz(d'1999-12-31');
dcl-s t time inz(t'14.30.05');
dcl-s z timestamp inz(z'2026-03-05-14.30.05.123456');
dcl-s z0 timestamp(0) inz(z'2026-03-05-14.30.05');
dcl-s n6 packed(6:0);
dcl-s n8 zoned(8:0);
line = %char(%dec(d:*iso)) + ' ' + %char(%dec(d:*usa)) + ' ' + %char(%dec(d:*eur)) + ' ' +
      %char(%dec(d:*jis));
write QSYSPRT line;
line = %char(%dec(d:*mdy)) + ' ' + %char(%dec(d:*dmy)) + ' ' + %char(%dec(d:*ymd)) + ' ' +
      %char(%dec(d:*jul));
write QSYSPRT line;
line = %char(%dec(d:*cymd)) + ' ' + %char(%dec(d:*cmdy)) + ' ' + %char(%dec(d:*cdmy));
write QSYSPRT line;
line = %char(%dec(d:*longjul)) + ' ' + %char(%dec(d)) + ' ' + %char(%dec(e)) + ' ' +
      %char(%dec(e:*cymd));
write QSYSPRT line;
line = %char(%dec(t:*hms)) + ' ' + %char(%dec(t:*iso)) + ' ' + %char(%dec(t));
write QSYSPRT line;
line = %char(%dec(z)) + ' ' + %char(%dec(z0));
write QSYSPRT line;
n6 = %dec(d:*ymd);
n8 = %dec(e:*usa);
line = %char(n6) + ' ' + %char(n8) + ' ' + %char(%dec(d:*ymd) + 1);
write QSYSPRT line;
*inlr = *on;
