**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s t time inz(t'14.30.05');
dcl-s u time inz(t'00.05.09');
dcl-s n time inz(t'12.00.00');
dcl-s h time(*hms) inz(t'09.08.07');
dcl-s z timestamp inz(z'2026-03-05-14.30.05.123456');
dcl-s z0 timestamp(0) inz(z'2026-03-05-14.30.05');
dcl-s z12 timestamp(12) inz(z'2026-03-05-14.30.05.123456789012');
line = %char(t:*hms) + ' ' + %char(t:*iso) + ' ' + %char(t:*usa) + ' ' + %char(t:*eur) +
      ' ' + %char(t:*jis);
write QSYSPRT line;
line = %char(u:*usa) + ' ' + %char(n:*usa) + ' ' + %char(t:*hms0) + ' ' + %char(t:*iso0);
write QSYSPRT line;
line = %char(t:*hms.) + ' ' + %char(t:*hms,) + ' ' + %char(t:*hms&) + '|' + %char(h);
write QSYSPRT line;
line = %char(z) + ' ' + %char(z:*iso0);
write QSYSPRT line;
line = %char(z0) + ' ' + %char(z12);
write QSYSPRT line;
line = %char(z12:*iso0) + ' ' + %char(z0:*iso0);
write QSYSPRT line;
*inlr = *on;
