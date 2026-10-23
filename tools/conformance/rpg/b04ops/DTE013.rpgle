**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s d date inz(d'2026-03-05');
dcl-s e date(*mdy) inz(d'2026-03-05');
dcl-s j date(*jul) inz(d'1999-12-31');
dcl-s n packed(9:0);
line = %char(%subdt(d:*years)) + ' ' + %char(%subdt(d:*months)) + ' ' +
      %char(%subdt(d:*days));
write QSYSPRT line;
line = %char(%subdt(e:*y)) + ' ' + %char(%subdt(e:*m)) + ' ' + %char(%subdt(e:*d));
write QSYSPRT line;
line = %char(%subdt(j:*y)) + ' ' + %char(%subdt(j:*m)) + ' ' + %char(%subdt(j:*d));
write QSYSPRT line;
n = %subdt(d:*y) * 10000 + %subdt(d:*m) * 100 + %subdt(d:*d);
line = %char(n);
write QSYSPRT line;
line = %char(%subdt(d'2024-02-29' + %days(1):*days)) + ' ' +
      %char(%subdt(z'2026-03-05-01.02.03':*mn));
write QSYSPRT line;
*inlr = *on;
