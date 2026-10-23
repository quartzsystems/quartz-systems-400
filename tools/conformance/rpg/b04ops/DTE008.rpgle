**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s n6 zoned(6:0) inz(143005);
dcl-s n20 packed(20:0) inz(20260305143005123456);
line = %char(%time('14.30.05':*iso)) + ' ' + %char(%time('14:30:05':*hms));
write QSYSPRT line;
line = %char(%time('02:30 PM':*usa)) + ' ' + %char(%time('12:15 AM':*usa));
write QSYSPRT line;
line = %char(%time(n6:*hms)) + ' ' + %char(%time('143005':*hms0)) + ' ' +
      %char(%time(143005));
write QSYSPRT line;
line = %char(%time(z'2026-03-05-14.30.05.123456'));
write QSYSPRT line;
line = %char(%timestamp('2026-03-05-14.30.05.123456'));
write QSYSPRT line;
line = %char(%timestamp(n20)) + ' ' + %char(%timestamp(d'2026-03-05'));
write QSYSPRT line;
line = %char(%timestamp('20260305143005123456':*iso0));
write QSYSPRT line;
*inlr = *on;
