**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s a char(4) dim(3) ctdata;
dcl-s b zoned(3:1) dim(6) ctdata perrcd(3);
dcl-s c date(*iso) dim(2) ctdata;
dcl-s d char(2) dim(5) ctdata perrcd(2);
line = a(1) + a(2) + a(3);
write QSYSPRT line;
line = %char(b(1)) + ' ' + %char(b(3)) + ' ' + %char(b(6));
write QSYSPRT line;
line = %char(c(1)) + ' ' + %char(c(2));
write QSYSPRT line;
line = d(1) + d(2) + d(3) + d(4) + '[' + d(5) + ']';
write QSYSPRT line;
*inlr = *on;
**CTDATA a
one
two
thre
**CTDATA b
010025999
100200300
**CTDATA c
2026-01-31
1999-12-31
**CTDATA d
aabb
ccdd
ee
