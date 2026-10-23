**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s e date(*mdy) inz(d'2026-03-05');
line = %char(%diff(d'2026-03-05':d'2025-03-05':*days)) + ' ' +
      %char(%diff(d'2026-03-05':d'2025-03-05':*months));
write QSYSPRT line;
line = %char(%diff(d'2026-03-05':d'2025-03-05':*years)) + ' ' +
      %char(%diff(d'2026-03-04':d'2025-03-05':*years));
write QSYSPRT line;
line = %char(%diff(d'2025-03-05':d'2026-03-04':*months)) + ' ' +
      %char(%diff(d'2025-03-05':d'2026-03-04':*y));
write QSYSPRT line;
line = %char(%diff(d'2024-02-29':d'2024-01-31':*months)) + ' ' +
      %char(%diff(d'2024-03-31':d'2024-02-29':*m));
write QSYSPRT line;
line = %char(%diff(d'2024-01-31':d'2024-02-29':*months)) + ' ' +
      %char(%diff(d'2025-02-28':d'2024-02-29':*y));
write QSYSPRT line;
line = %char(%diff(d'2026-01-01':d'1940-01-01':*days)) + ' ' +
      %char(%diff(e:d'2026-01-01':*d));
write QSYSPRT line;
line = %char(%diff(d'2026-01-01':d'2026-03-05':*days)) + ' ' +
      %char(%diff(e:d'2025-03-06':*years));
write QSYSPRT line;
*inlr = *on;
