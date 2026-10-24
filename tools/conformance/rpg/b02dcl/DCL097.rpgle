**FREE
ctl-opt dftactgrp(*no) datfmt(*mdy-) timfmt(*usa);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s d1 date inz(d'03-05-26');
dcl-s d2 date(*iso) inz(d'12-31-39');
dcl-s t1 time inz(t'02:30 PM');
dcl-s t2 time(*iso) inz(t'11:15 AM');
dcl-s ts timestamp inz(z'2026-03-05-14.30.05.123456');
line = %char(d1) + ' ' + %char(d2) + ' ' + %char(t1) + ' ' + %char(t2);
write QSYSPRT line;
line = %char(%size(d1)) + ' ' + %char(ts);
write QSYSPRT line;
*inlr = *on;
